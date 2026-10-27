//! Signed, channel-scoped blue/green deployment verifier.
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use buzz_ws_client::{
    message::build_auth_event, HeaderMap, HeaderValue, NostrWsConnection, RelayMessage,
};
use nostr::{EventBuilder, Keys, Kind, Tag};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    canonical_url: String,
    endpoints: [String; 2],
    private_key: String,
    channel: String,
}
struct Actor {
    connection: NostrWsConnection,
    observed: BTreeSet<String>,
    connection_id: String,
}
struct Ledger {
    file: File,
    accepted: Vec<String>,
    observed: BTreeSet<String>,
    reconnect_ms: Vec<u128>,
    uncertain: Vec<String>,
}

fn verify_ids(accepted: &[String], fanout: &[String], replay: &[String]) -> Result<()> {
    let accepted: BTreeSet<_> = accepted.iter().collect();
    ensure!(
        accepted.is_subset(&fanout.iter().collect()),
        "accepted IDs missing from observer"
    );
    ensure!(
        accepted.is_subset(&replay.iter().collect()),
        "accepted IDs missing from durable replay"
    );
    Ok(())
}
impl Ledger {
    fn record(&mut self, value: Value) -> Result<()> {
        serde_json::to_writer(&mut self.file, &value)?;
        self.file.write_all(b"\n")?;
        self.file.sync_data()?;
        Ok(())
    }
}
async fn connect(config: &Config, endpoint: &str, keys: &Keys, subscribe: bool) -> Result<Actor> {
    let canonical = url::Url::parse(&config.canonical_url)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "host",
        HeaderValue::from_str(&canonical[url::Position::BeforeHost..url::Position::AfterPort])?,
    );
    let mut connection = NostrWsConnection::connect_with_headers(endpoint, headers).await?;
    let challenge = match connection.next_event(Duration::from_secs(5)).await? {
        RelayMessage::Auth { challenge } => challenge,
        _ => bail!("missing AUTH challenge"),
    };
    let auth = build_auth_event(&challenge, &config.canonical_url, keys, None)?;
    let auth_id = auth.id.to_hex();
    connection.send_raw(&json!(["AUTH", auth])).await?;
    match connection.next_event(Duration::from_secs(5)).await? {
        RelayMessage::Ok(ok) if ok.accepted && ok.event_id == auth_id => {}
        _ => bail!("AUTH rejected"),
    }
    if !subscribe {
        return Ok(Actor {
            connection,
            observed: BTreeSet::new(),
            connection_id: Uuid::new_v4().to_string(),
        });
    }
    connection
        .send_raw(&json!(["REQ", "live", {"kinds":[9],"#h":[config.channel],"limit":0}]))
        .await?;
    loop {
        match connection.next_event(Duration::from_secs(5)).await? {
            RelayMessage::Eose { subscription_id } if subscription_id == "live" => break,
            RelayMessage::Closed { .. } => bail!("subscription denied"),
            _ => {}
        }
    }
    Ok(Actor {
        connection,
        observed: BTreeSet::new(),
        connection_id: Uuid::new_v4().to_string(),
    })
}
async fn observe(actor: &mut Actor, id: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !actor.observed.contains(id) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "fanout deadline exceeded");
        match actor.connection.next_event(remaining).await? {
            RelayMessage::Event { event, .. } => {
                actor.observed.insert(event.id.to_hex());
            }
            RelayMessage::Closed { .. } => bail!("observer subscription closed"),
            _ => {}
        }
    }
    Ok(())
}
fn http_url(ws: &str) -> String {
    ws.replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
        .trim_end_matches('/')
        .to_string()
}
async fn query(
    client: &reqwest::Client,
    config: &Config,
    endpoint: &str,
    keys: &Keys,
    ids: &[String],
) -> Result<Vec<String>> {
    let body =
        serde_json::to_vec(&json!([{"kinds":[9],"#h":[config.channel],"ids":ids,"limit":64}]))?;
    let canonical = http_url(&config.canonical_url) + "/query";
    let payload = hex::encode(Sha256::digest(&body));
    let nonce = Uuid::new_v4().to_string();
    let proof = EventBuilder::new(Kind::Custom(27235), "")
        .tags([
            Tag::parse(["u", &canonical])?,
            Tag::parse(["method", "POST"])?,
            Tag::parse(["payload", &payload])?,
            Tag::parse(["probe", &nonce])?,
        ])
        .sign_with_keys(keys)?;
    let host = url::Url::parse(&canonical)?;
    let response = client
        .post(http_url(endpoint) + "/query")
        .header(
            "Host",
            &host[url::Position::BeforeHost..url::Position::AfterPort],
        )
        .header(
            "Authorization",
            "Nostr ".to_string() + &STANDARD.encode(serde_json::to_vec(&proof)?),
        )
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "signed query rejected: {}",
        response.status()
    );
    let result: Value = response.json().await?;
    let events = result
        .as_array()
        .or_else(|| result.get("events").and_then(Value::as_array))
        .context("invalid query response")?;
    events
        .iter()
        .map(|event| {
            event["id"]
                .as_str()
                .map(str::to_owned)
                .context("query event has no ID")
        })
        .collect()
}
async fn reconcile(
    client: &reqwest::Client,
    config: &Config,
    keys: &Keys,
    accepted: &[String],
) -> Result<Vec<String>> {
    for endpoint in &config.endpoints {
        let mut found = Vec::new();
        for chunk in accepted.chunks(64) {
            found.extend(query(client, config, endpoint, keys, chunk).await?);
        }
        verify_ids(accepted, accepted, &found)?;
        let mut observer = connect(config, endpoint, keys, true).await?;
        let mut replay = Vec::new();
        for (index, chunk) in accepted.chunks(64).enumerate() {
            let subscription = format!("replay-{index}");
            observer.connection.send_raw(&json!(["REQ", subscription, {"kinds":[9],"#h":[config.channel],"ids":chunk,"limit":128}])).await?;
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                ensure!(!remaining.is_zero(), "replay deadline exceeded");
                match observer.connection.next_event(remaining).await? {
                    RelayMessage::Event {
                        subscription_id,
                        event,
                    } if subscription_id == subscription => replay.push(event.id.to_hex()),
                    RelayMessage::Eose { subscription_id } if subscription_id == subscription => {
                        break
                    }
                    RelayMessage::Closed {
                        subscription_id, ..
                    } if subscription_id == subscription => bail!("replay subscription denied"),
                    _ => {}
                }
            }
            observer
                .connection
                .send_raw(&json!(["CLOSE", subscription]))
                .await?;
        }
        observer.connection.disconnect().await?;
        verify_ids(accepted, accepted, &replay)?;
    }
    Ok(accepted.to_vec())
}
async fn exchange(
    config: &Config,
    keys: &Keys,
    actors: &mut [Actor; 2],
    ledger: &mut Ledger,
) -> Result<()> {
    for writer in 0..2 {
        let event = EventBuilder::new(
            Kind::Custom(9),
            format!("deployment probe {}", Uuid::new_v4()),
        )
        .tags([Tag::parse(["h", &config.channel])?])
        .sign_with_keys(keys)?;
        let id = event.id.to_hex();
        let sent = Instant::now();
        ledger.record(json!({"type":"sent","id":id,"writer":writer,"connection_id":actors[writer].connection_id}))?;
        match actors[writer].connection.send_event(event).await {
            Ok(ok) if ok.accepted => {
                ledger.accepted.push(id.clone());
                ledger.record(json!({"type":"accepted","id":id,"writer":writer,"ack_ms":sent.elapsed().as_millis()}))?;
            }
            Ok(_) => bail!("persistent probe event rejected"),
            Err(error) => {
                ledger.uncertain.push(id.clone());
                ledger.record(json!({"type":"uncertain","id":id,"writer":writer}))?;
                return Err(error.into());
            }
        }
        observe(&mut actors[1 - writer], &id).await?;
        ledger.observed.insert(id.clone());
        ledger.record(json!({"type":"observed","id":id,"observer":1-writer,"elapsed_ms":sent.elapsed().as_millis()}))?;
    }
    Ok(())
}
async fn run(config: Config, output: &Path, mode: &str, duration: u64) -> Result<()> {
    let keys = Keys::parse(&config.private_key)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(output.join("ids.jsonl"))?;
    let mut ledger = Ledger {
        file,
        accepted: Vec::new(),
        observed: BTreeSet::new(),
        reconnect_ms: Vec::new(),
        uncertain: Vec::new(),
    };
    let mut actors = [
        connect(&config, &config.endpoints[0], &keys, mode != "bootstrap").await?,
        connect(&config, &config.endpoints[1], &keys, mode != "bootstrap").await?,
    ];
    if mode == "bootstrap" {
        let event = EventBuilder::new(Kind::Custom(9007), "")
            .tags([
                Tag::parse(["h", &config.channel])?,
                Tag::parse(["name", "deployment-verification"])?,
                Tag::parse(["channel_type", "stream"])?,
                Tag::parse(["visibility", "private"])?,
            ])
            .sign_with_keys(&keys)?;
        ensure!(
            actors[0].connection.send_event(event).await?.accepted,
            "channel creation rejected"
        );
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(duration);
    let mut round = 0;
    loop {
        let tick = Instant::now();
        if mode != "continuous" || writes_on_round(round) {
            if let Err(error) = exchange(&config, &keys, &mut actors, &mut ledger).await {
                if mode != "continuous" {
                    return Err(error);
                }
                let started = tick;
                loop {
                    match (
                        connect(&config, &config.endpoints[0], &keys, mode != "bootstrap").await,
                        connect(&config, &config.endpoints[1], &keys, mode != "bootstrap").await,
                    ) {
                        (Ok(a), Ok(b)) => {
                            actors = [a, b];
                            break;
                        }
                        _ if started.elapsed() < Duration::from_secs(60) => {
                            tokio::time::sleep(Duration::from_millis(100)).await
                        }
                        _ => bail!("reconnect exceeded 60 seconds"),
                    }
                }
                ledger.reconnect_ms.push(started.elapsed().as_millis());
                let replay = reconcile(&client, &config, &keys, &ledger.accepted).await?;
                ledger.observed.extend(replay);
                ledger.record(
                    json!({"type":"reconnected","milliseconds":started.elapsed().as_millis()}),
                )?;
            }
        }
        if let Some(id) = ledger.accepted.last() {
            let found = query(
                &client,
                &config,
                &config.endpoints[0],
                &keys,
                std::slice::from_ref(id),
            )
            .await?;
            verify_ids(std::slice::from_ref(id), std::slice::from_ref(id), &found)?;
        }
        if round % 2 == 0 {
            let short = connect(&config, &config.endpoints[1], &keys, mode != "bootstrap").await?;
            short.connection.disconnect().await?;
        }
        round += 1;
        if mode == "gate" || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500).saturating_sub(tick.elapsed())).await;
    }
    for actor in actors {
        actor.connection.disconnect().await?;
    }
    let replay = reconcile(&client, &config, &keys, &ledger.accepted).await?;
    verify_ids(
        &ledger.accepted,
        &ledger.observed.iter().cloned().collect::<Vec<_>>(),
        &replay,
    )?;
    let mut uncertain_visible = 0;
    for chunk in ledger.uncertain.chunks(64) {
        uncertain_visible += query(&client, &config, &config.endpoints[0], &keys, chunk)
            .await?
            .len();
    }
    ledger.reconnect_ms.sort_unstable();
    let p99 = ledger
        .reconnect_ms
        .get(ledger.reconnect_ms.len().saturating_mul(99) / 100)
        .copied()
        .unwrap_or(0);
    let summary = json!({"status":if p99<=5000 {"passed"} else {"failed"},"accepted":ledger.accepted.len(),"missing":0,"observed":ledger.observed.len(),"uncertain":ledger.uncertain.len(),"uncertain_visible":uncertain_visible,"reconnect_count":ledger.reconnect_ms.len(),"reconnect_p99_ms":p99,"mode":mode,"rounds":round});
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    ensure!(p99 <= 5000, "reconnect p99 exceeded five seconds");
    Ok(())
}
#[tokio::main]
pub(super) async fn main() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 5,
        "usage: blue_green_probe CONFIG OUTPUT MODE DURATION_SECONDS"
    );
    let config_path = Path::new(&args[1]);
    ensure!(
        config_path.metadata()?.permissions().mode() & 0o077 == 0,
        "config must be owner-only"
    );
    ensure!(
        matches!(args[3].as_str(), "gate" | "continuous" | "bootstrap"),
        "unknown probe mode"
    );
    let config: Config = serde_json::from_slice(&std::fs::read(config_path)?)?;
    let output = Path::new(&args[2]);
    std::fs::create_dir_all(output)?;
    let duration: u64 = args[4].parse()?;
    ensure!(duration <= 14400, "probe duration exceeds four hours");
    run(config, output, &args[3], duration).await
}
// Two events per exchange, once per five 500 ms ticks: at most 48/minute.
// Leave the remaining principal quota for bootstrap and deployment gates.
fn writes_on_round(round: u64) -> bool {
    round.is_multiple_of(5)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_ack_alone_cannot_pass_without_fanout_and_replay() {
        assert!(verify_ids(&["a".into()], &["a".into()], &["a".into()]).is_ok());
        assert!(verify_ids(&["a".into()], &[], &["a".into()]).is_err());
        assert!(verify_ids(&["a".into()], &["a".into()], &[]).is_err());
    }

    #[test]
    fn continuous_writes_leave_room_for_gates_under_the_default_principal_quota() {
        let writes = (0..120).filter(|round| writes_on_round(*round)).count() * 2;
        assert_eq!(writes, 48);
        assert!(writes + 9 <= 60); // Two gates in each direction and channel creation.
    }
}
