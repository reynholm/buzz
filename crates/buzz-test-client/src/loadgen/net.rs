//! Connection helpers that honor the scenario's `connect_addr` override.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

use buzz_ws_client::NostrWsConnection;
use nostr::{Event, JsonUtil, Keys, Tag};

use super::identity;
use super::scenario::{Community, Scenario};

/// Shared endpoints for one shard. When `connect_addr` is set, community hosts
/// resolve to it and are dialled in plaintext there, while requests are still
/// signed for the community's own `https://` and `wss://` URLs.
#[derive(Clone)]
pub struct Net {
    http: reqwest::Client,
    /// Explicit community destinations bypass ambient proxies; operator and
    /// ordinary requests keep the normal client's proxy policy.
    direct_http: Option<reqwest::Client>,
    connect_addr: Option<String>,
    /// Community hosts that resolve to `connect_addr`.
    plaintext_hosts: HashSet<String>,
    connect_timeout: Duration,
}

impl Net {
    pub async fn new(scenario: &Scenario) -> anyhow::Result<Self> {
        let builder = || {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .pool_max_idle_per_host(1024)
        };
        let mut plaintext_hosts = HashSet::new();
        let direct_http = if let Some(addr) = &scenario.target.connect_addr {
            let mut direct = builder().no_proxy();
            let resolved: SocketAddr = tokio::net::lookup_host(addr)
                .await?
                .next()
                .ok_or_else(|| anyhow::anyhow!("connect_addr {addr} did not resolve"))?;
            for community in scenario.communities() {
                if let Some(host) = community.host {
                    direct = direct.resolve(&host, resolved);
                    plaintext_hosts.insert(host);
                }
            }
            Some(direct.build()?)
        } else {
            None
        };
        Ok(Self {
            http: builder().build()?,
            direct_http,
            connect_addr: scenario.target.connect_addr.clone(),
            plaintext_hosts,
            connect_timeout: Duration::from_secs_f64(scenario.target.connect_timeout_secs),
        })
    }

    /// Opens a WebSocket to `community` and authenticates as `keys`, failing
    /// once the whole attempt exceeds `target.connect_timeout_secs`.
    pub async fn connect(
        &self,
        community: &Community,
        keys: &Keys,
        auth_tag: Option<&Tag>,
    ) -> anyhow::Result<NostrWsConnection> {
        let attempt = async {
            let mut conn = match (&self.connect_addr, &community.host) {
                (Some(addr), Some(_)) => {
                    NostrWsConnection::connect_to_addr(&community.ws_url, addr).await?
                }
                _ => NostrWsConnection::connect(&community.ws_url).await?,
            };
            conn.authenticate(keys, auth_tag).await?;
            anyhow::Ok(conn)
        };
        tokio::time::timeout(self.connect_timeout, attempt)
            .await
            .map_err(|_| anyhow::anyhow!("connect timeout after {:?}", self.connect_timeout))?
    }

    /// The URL to send a request signed for `url` to: plain `http://` for a
    /// community host dialled at `connect_addr`, otherwise `url` itself.
    fn transport_url(&self, url: &str) -> String {
        match url.strip_prefix("https://") {
            Some(rest) if self.is_mapped_host(url) => {
                format!("http://{rest}")
            }
            _ => url.to_string(),
        }
    }

    fn is_mapped_host(&self, url: &str) -> bool {
        url.split_once("://").is_some_and(|(_, rest)| {
            self.plaintext_hosts
                .contains(rest.split(['/', '?']).next().unwrap_or_default())
        })
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let client = self
            .direct_http
            .as_ref()
            .filter(|_| self.is_mapped_host(url))
            .unwrap_or(&self.http);
        client.request(method, self.transport_url(url))
    }

    /// `POST {base}{path}` with a NIP-98 header; returns status and body.
    pub async fn post(
        &self,
        keys: &Keys,
        base: &str,
        path: &str,
        body: Vec<u8>,
        auth_tag: Option<&Tag>,
    ) -> anyhow::Result<(reqwest::StatusCode, String)> {
        let url = format!("{base}{path}");
        let mut request = self
            .request(reqwest::Method::POST, &url)
            .header(
                "Authorization",
                identity::nip98_header(keys, "POST", &url, &body),
            )
            .header("Content-Type", "application/json");
        if let Some(tag) = auth_tag {
            request = request.header("x-auth-tag", serde_json::to_string(tag.as_slice())?);
        }
        let response = request.body(body).send().await?;
        let status = response.status();
        Ok((status, response.text().await?))
    }

    /// `GET {url}` with a NIP-98 header; returns status and body.
    pub async fn get(
        &self,
        keys: &Keys,
        url: &str,
    ) -> anyhow::Result<(reqwest::StatusCode, String)> {
        let response = self
            .request(reqwest::Method::GET, url)
            .header(
                "Authorization",
                identity::nip98_header(keys, "GET", url, &[]),
            )
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.text().await?))
    }

    /// `GET {base}/` for the relay's NIP-11 information document.
    pub async fn relay_info(&self, base: &str) -> anyhow::Result<(reqwest::StatusCode, String)> {
        let response = self
            .request(reqwest::Method::GET, &format!("{base}/"))
            .header("Accept", "application/nostr+json")
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.text().await?))
    }

    /// Submits `event` through HTTP `/events`; errors unless the relay accepted it.
    pub async fn submit(
        &self,
        keys: &Keys,
        community: &Community,
        event: &Event,
        auth_tag: Option<&Tag>,
    ) -> anyhow::Result<()> {
        let (status, body) = self
            .post(
                keys,
                &community.http_url,
                "/events",
                event.as_json().into_bytes(),
                auth_tag,
            )
            .await?;
        if !status.is_success() {
            anyhow::bail!("{}", http_reason(status, &body));
        }
        let value: serde_json::Value = serde_json::from_str(&body)?;
        if value["accepted"].as_bool() != Some(true) {
            anyhow::bail!("rejected: {}", value["message"].as_str().unwrap_or(&body));
        }
        Ok(())
    }
}

pub fn truncate(s: &str) -> String {
    s.chars().take(200).collect()
}

/// Failure text for a non-success HTTP response. It starts `HTTP <status>`,
/// which `stats` reads to tell a 429 from every other failure.
pub fn http_reason(status: reqwest::StatusCode, body: &str) -> String {
    format!("HTTP {status}: {}", truncate(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A local proxy makes routing observable without depending on the
    /// operator's proxy configuration or mutating the parent test environment.
    #[tokio::test]
    async fn connect_addr_bypasses_proxy_only_for_mapped_community_hosts() {
        const CHILD: &str = "BUZZ_TEST_LOADGEN_PROXY_CHILD";
        const NAME: &str =
            "loadgen::net::tests::connect_addr_bypasses_proxy_only_for_mapped_community_hosts";
        if std::env::var_os(CHILD).is_none() {
            use std::sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            };
            use tokio::io::AsyncReadExt;
            let calls = Arc::new(AtomicUsize::new(0));
            let observed = calls.clone();
            let proxy = crate::loadgen::seed::tests::fake_http(move |_, _, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                (200, "proxy".to_owned())
            })
            .await;
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", NAME, "--nocapture"])
                .env(CHILD, "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            for key in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ] {
                command.env(key, format!("http://{proxy}"));
            }
            command.env("NO_PROXY", "").env("no_proxy", "");
            let mut child = command.spawn().unwrap();
            let capture = |reader: Option<tokio::process::ChildStdout>| async move {
                let mut bytes = Vec::new();
                reader
                    .unwrap()
                    .take(65_537)
                    .read_to_end(&mut bytes)
                    .await
                    .unwrap();
                bytes
            };
            let stdout = tokio::spawn(capture(child.stdout.take()));
            let stderr = child.stderr.take().unwrap();
            let stderr = tokio::spawn(async move {
                let mut bytes = Vec::new();
                stderr.take(65_537).read_to_end(&mut bytes).await.unwrap();
                bytes
            });
            let status = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
            let timed_out = status.is_err();
            let status = match status {
                Ok(status) => status.unwrap(),
                Err(_) => {
                    child.kill().await.unwrap();
                    child.wait().await.unwrap()
                }
            };
            let stdout = stdout.await.unwrap();
            let stderr = stderr.await.unwrap();
            assert!(
                stdout.len() <= 65_536 && stderr.len() <= 65_536,
                "proxy child exceeded its capture budget"
            );
            let output = format!(
                "{}{}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
            assert!(!timed_out, "proxy child exceeded its deadline: {output}");
            assert!(status.success(), "proxy child failed: {output}");
            assert!(
                output.contains("running 1 test") && output.contains(NAME),
                "{output}"
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                4,
                "only the operator and ordinary requests use the proxy"
            );
            return;
        }
        let direct =
            crate::loadgen::seed::tests::fake_http(|_, _, _| (200, "direct".to_owned())).await;
        let scenario = Scenario::parse(&format!(
            r#"
run_id = "proxy"
[target]
ws_url = "wss://public.example"
http_url = "https://public.example"
community_host_template = "lg-{{run}}-{{i}}.example"
connect_addr = "{direct}"
[shape]
communities = 1
channels_per_community = 1
[population]
humans = 1
agents = 0
channels_per_member = 1
[[ramp]]
users = 1.0
hold_secs = 1
"#
        ))
        .unwrap();
        let net = Net::new(&scenario).await.unwrap();
        let community = scenario.communities().remove(0);
        let keys = Keys::generate();
        for base in [
            community.http_url.clone(),
            community.http_url.replacen("https://", "http://", 1),
        ] {
            let (status, body) = net
                .post(&keys, &base, "/events", b"{}".to_vec(), None)
                .await
                .unwrap();
            assert!(status.is_success(), "{status}: {body}");
            assert_eq!(
                body, "direct",
                "connect_addr must select the actual HTTP destination"
            );
            assert_eq!(
                net.get(&keys, &format!("{base}/read")).await.unwrap().1,
                "direct"
            );
            assert_eq!(net.relay_info(&base).await.unwrap().1, "direct");
        }
        assert_eq!(
            net.post(
                &keys,
                "http://operator.example",
                "/operator/communities",
                b"{}".to_vec(),
                None
            )
            .await
            .unwrap()
            .1,
            "proxy"
        );
        assert_eq!(
            net.get(&keys, "http://operator.example/policy")
                .await
                .unwrap()
                .1,
            "proxy"
        );
        assert_eq!(
            net.relay_info("http://operator.example").await.unwrap().1,
            "proxy"
        );
        let mut ordinary = scenario.clone();
        ordinary.target.connect_addr = None;
        ordinary.target.community_host_template = None;
        ordinary.target.http_url = "http://ordinary.example".to_owned();
        ordinary.target.ws_url = "ws://ordinary.example".to_owned();
        let net = Net::new(&ordinary).await.unwrap();
        assert_eq!(
            net.get(&keys, "http://ordinary.example/read")
                .await
                .unwrap()
                .1,
            "proxy"
        );
    }

    #[tokio::test]
    async fn stalled_upgrade_fails_at_the_connect_deadline() {
        // A peer that accepts TCP but never answers the WebSocket upgrade.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let held = tokio::spawn(async move {
            let mut open = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                open.push(socket);
            }
        });
        let scenario = Scenario::parse(&format!(
            r#"
run_id = "t"
[target]
ws_url = "ws://{addr}"
http_url = "http://{addr}"
connect_timeout_secs = 0.3
[shape]
communities = 1
channels_per_community = 1
[population]
humans = 1
agents = 0
channels_per_member = 1
[[ramp]]
users = 1.0
hold_secs = 1
"#
        ))
        .unwrap();
        let net = Net::new(&scenario).await.unwrap();
        let community = &scenario.communities()[0];
        let started = std::time::Instant::now();
        let err = tokio::time::timeout(
            Duration::from_secs(5),
            net.connect(community, &Keys::generate(), None),
        )
        .await
        .expect("connect returns at its own deadline")
        .err()
        .expect("a silent peer cannot complete the upgrade");
        assert!(err.to_string().contains("connect timeout"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
        held.abort();
    }

    /// The tag value named `name` in `event`.
    fn tag(event: &Event, name: &str) -> String {
        event
            .tags
            .iter()
            .find(|t| t.as_slice()[0] == name)
            .map(|t| t.as_slice()[1].clone())
            .unwrap_or_else(|| panic!("no {name} tag"))
    }

    #[tokio::test]
    // tungstenite's handshake callback returns its large error response by value.
    #[allow(clippy::result_large_err)]
    async fn connect_addr_dials_plaintext_but_signs_the_public_urls() {
        use base64::Engine as _;
        use futures_util::{SinkExt, StreamExt};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_tungstenite::tungstenite::{handshake::server, Message};

        // A plaintext listener standing in for the relay's in-cluster service,
        // while the scenario names the community's public wss:// deployment.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let scenario = Scenario::parse(&format!(
            r#"
run_id = "t"
[target]
ws_url = "wss://public.example"
http_url = "https://public.example"
community_host_template = "lg-{{run}}-{{i}}.example"
connect_addr = "{addr}"
[shape]
communities = 1
channels_per_community = 1
[population]
humans = 1
agents = 0
channels_per_member = 1
[[ramp]]
users = 1.0
hold_secs = 1
"#
        ))
        .unwrap();
        let net = Net::new(&scenario).await.unwrap();
        let community = scenario.communities().remove(0);
        assert_eq!(community.http_url, "https://lg-t-0.example");
        let keys = Keys::generate();

        // HTTP: plaintext on the wire, Host names the community, and the
        // NIP-98 `u` tag is the https:// URL the relay derives from a wss://
        // relay URL (`{scheme}://{host}{path}`).
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0, "connection closed before the request headers");
                request.extend_from_slice(&buf[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            (
                String::from_utf8(request).expect("plaintext HTTP"),
                listener,
            )
        });
        let (status, _) = net
            .post(&keys, &community.http_url, "/events", b"{}".to_vec(), None)
            .await
            .unwrap();
        assert!(status.is_success());
        let (request, listener) = server.await.unwrap();
        let header = |name: &str| {
            request
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case(name)
                        .then(|| value.trim().to_string())
                })
                .unwrap_or_else(|| panic!("no {name} header in {request}"))
        };
        assert!(request.starts_with("POST /events "), "{request}");
        assert_eq!(header("host"), "lg-t-0.example");
        let token = header("authorization");
        let json = base64::engine::general_purpose::STANDARD
            .decode(token.strip_prefix("Nostr ").unwrap())
            .unwrap();
        let nip98 = Event::from_json(json).unwrap();
        assert_eq!(tag(&nip98, "u"), "https://lg-t-0.example/events");

        // WebSocket: plaintext upgrade with the community's Host, and a NIP-42
        // relay tag of wss://<community host>.
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut host = None;
            let mut ws = tokio_tungstenite::accept_hdr_async(
                socket,
                |req: &server::Request, resp: server::Response| {
                    host = req
                        .headers()
                        .get("host")
                        .map(|h| h.to_str().unwrap().to_string());
                    Ok(resp)
                },
            )
            .await
            .expect("plaintext WebSocket upgrade");
            ws.send(Message::text(r#"["AUTH","challenge"]"#))
                .await
                .unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else {
                panic!("no AUTH reply");
            };
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            let auth = Event::from_json(frame[1].to_string()).unwrap();
            let ok = serde_json::json!(["OK", auth.id.to_hex(), true, ""]);
            ws.send(Message::text(ok.to_string())).await.unwrap();
            (host, auth)
        });
        net.connect(&community, &keys, None).await.unwrap();
        let (host, auth) = server.await.unwrap();
        assert_eq!(host.as_deref(), Some("lg-t-0.example"));
        assert_eq!(
            tag(&auth, "relay").trim_end_matches('/'),
            "wss://lg-t-0.example"
        );
    }
}
