//! Delivery-only completion and restart replay; no provider/session entrypoints.
use crate::{managed_work, pool::PromptOutcome, queue::LogicalWork, relay::RestClient};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeliveryOutcome {
    AnswerConfirmed,
    FailureNoticeConfirmed,
    DeliveryPending,
    MissingFinal,
    Cancelled,
}

/// Complete the accepted turn, retaining its immutable work/key/route snapshot.
/// Called only for terminal outcomes; authorized control continuations bypass
/// completion and retain their original receipt. No provider retry obtains a reply.
pub(crate) async fn complete(
    rest: &RestClient,
    work: &LogicalWork,
    outcome: &PromptOutcome,
    final_text: Result<String, &'static str>,
) -> DeliveryOutcome {
    complete_using(
        &managed_work::delivery_command(),
        rest,
        work,
        outcome,
        final_text,
    )
    .await
}

pub(crate) async fn complete_using(
    program: &Path,
    rest: &RestClient,
    work: &LogicalWork,
    outcome: &PromptOutcome,
    final_text: Result<String, &'static str>,
) -> DeliveryOutcome {
    let needs_user = matches!(
        outcome,
        PromptOutcome::Error(
            crate::acp::AcpError::HumanApprovalRequired
                | crate::acp::AcpError::HumanApprovalWorkerFailure(_)
        )
    );
    let cancelled = work.publication_revoked()
        || matches!(
            outcome,
            PromptOutcome::Cancelled
                | PromptOutcome::CancelDrainTimeout(_)
                | PromptOutcome::Ok(crate::acp::StopReason::Cancelled)
                | PromptOutcome::Error(crate::acp::AcpError::WorkCancelledPoisoned)
        );
    if cancelled {
        // A previous pending attempt may exist even when no text remains.
        if let Err(reason) = journal_command(
            program,
            rest,
            &work.journal_dir,
            "delivery-revoke",
            Some(work.delivery_key),
        )
        .await
        {
            tracing::warn!(work=%work.delivery_key,reason,"runtime revocation remains unconfirmed");
            emit(
                work,
                DeliveryOutcome::DeliveryPending,
                "runtime_revocation_unconfirmed",
            );
            work.finish();
            return DeliveryOutcome::DeliveryPending;
        }
        emit(work, DeliveryOutcome::Cancelled, "turn_cancelled");
        work.finish();
        return DeliveryOutcome::Cancelled;
    }
    if matches!(
        managed_work::confirmed_report(rest, work).await,
        Ok(Some(_))
    ) {
        let (outcome, reason) = match journal_command(
            program,
            rest,
            &work.journal_dir,
            "delivery-retry",
            Some(work.delivery_key),
        )
        .await
        {
            Ok(receipt) if receipt["accepted"] == true && receipt["purpose"] == "answer" => {
                (DeliveryOutcome::AnswerConfirmed, "saved_answer_confirmed")
            }
            Ok(receipt)
                if receipt["accepted"] == true && receipt["purpose"] == "failure_notice" =>
            {
                (
                    DeliveryOutcome::FailureNoticeConfirmed,
                    "saved_failure_notice_confirmed",
                )
            }
            Err(reason) => (DeliveryOutcome::DeliveryPending, reason),
            _ => (
                DeliveryOutcome::DeliveryPending,
                "runtime_receipt_purpose_unverified",
            ),
        };
        emit(work, outcome, reason);
        if needs_user {
            work.human_required("human_approval_required");
            work.transition("waiting_human", "human_approval_required");
        } else {
            work.finish();
        }
        return outcome;
    }
    let (content, purpose, reason) = match final_text {
        _ if matches!(outcome, PromptOutcome::Error(crate::acp::AcpError::HumanApprovalWorkerFailure(_))) => (
            "A human decision is required before the blocked action can start. The worker also failed; inspect its diagnostics before continuing. No action was approved and no automatic retry was scheduled.".into(),
            "failure_notice",
            "human_approval_worker_failure",
        ),
        _ if needs_user => (
            "A human decision is required before the blocked action can start. No action was approved and no automatic retry was scheduled.".into(),
            "failure_notice",
            "human_approval_required",
        ),
        Ok(text) if matches!(outcome, PromptOutcome::Ok(crate::acp::StopReason::EndTurn)) => {
            (text, "answer", "normal_end_turn")
        }
        result => {
            let reason = result.err().unwrap_or("turn_not_successful");
            emit(work, DeliveryOutcome::MissingFinal, reason);
            (format!("The agent did not produce a verified final answer ({reason}). Check the existing work before retrying; no provider action was replayed."),"failure_notice",reason)
        }
    };
    let (result, delivery_reason) =
        match managed_work::publish_runtime_using(program, rest, work, &content, purpose).await {
            Ok((_, Some(saved))) if saved == "answer" => (DeliveryOutcome::AnswerConfirmed, reason),
            Ok((_, Some(saved))) if saved == "failure_notice" => {
                (DeliveryOutcome::FailureNoticeConfirmed, reason)
            }
            Err("publication was explicitly revoked") => {
                (DeliveryOutcome::Cancelled, "publication_revoked")
            }
            Err(reason) => (DeliveryOutcome::DeliveryPending, reason),
            // A legacy receipt cannot certify runtime purpose.
            Ok(_) => (
                DeliveryOutcome::DeliveryPending,
                "runtime_receipt_purpose_unverified",
            ),
        };
    emit(work, result, delivery_reason);
    if needs_user {
        work.human_required("human_approval_required");
        work.transition("waiting_human", "human_approval_required");
    } else {
        work.finish();
    }
    result
}

fn emit(work: &LogicalWork, outcome: DeliveryOutcome, reason: &'static str) {
    let label = match outcome {
        DeliveryOutcome::AnswerConfirmed => "answer_confirmed",
        DeliveryOutcome::FailureNoticeConfirmed => "failure_notice_confirmed",
        DeliveryOutcome::DeliveryPending => "delivery_pending",
        DeliveryOutcome::MissingFinal => "missing_final",
        DeliveryOutcome::Cancelled => "cancelled",
    };
    work.transition(label, reason);
    tracing::info!(work=%work.delivery_key,scope=%work.scope.telemetry_label(),outcome=label,reason,"runtime reply delivery");
}

/// A bounded CLI operation. This passes only the namespace, directory and saved
/// UUID; the CLI alone validates records, checks membership and publishes.
pub(crate) async fn journal_command(
    program: &Path,
    rest: &RestClient,
    dir: &Path,
    operation: &str,
    key: Option<uuid::Uuid>,
) -> Result<serde_json::Value, &'static str> {
    journal_command_in_scope(program, rest, dir, operation, key, None).await
}

async fn journal_command_in_scope(
    program: &Path,
    rest: &RestClient,
    dir: &Path,
    operation: &str,
    key: Option<uuid::Uuid>,
    scope: Option<&crate::scope::SessionScope>,
) -> Result<serde_json::Value, &'static str> {
    let mut command = tokio::process::Command::new(program);
    command
        .env("BUZZ_RELAY_URL", &rest.base_url)
        .env("BUZZ_PRIVATE_KEY", rest.keys.secret_key().to_secret_hex())
        .env_remove("BUZZ_AUTH_TAG")
        .args(["messages", operation, "--journal-dir"])
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(auth) = &rest.auth_tag_json {
        command.env("BUZZ_AUTH_TAG", auth);
    }
    if let Some(key) = key {
        command.args(["--delivery-key", &key.to_string()]);
    }
    if let Some(scope) = scope {
        command.args(["--channel", &scope.channel_id().to_string()]);
        if let crate::scope::SessionScope::Thread { root_event_id, .. } = scope {
            command.args(["--thread-root", root_event_id]);
        }
    }
    let mut child = command.spawn().map_err(|_| "managed_cli_start_failed")?;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or("managed_cli_stdout_missing")?
            .take(65537)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "managed_cli_read_failed")?;
        if bytes.len() > 65536 {
            return Err("managed_cli_output_exceeded_bound");
        }
        let status = child.wait().await.map_err(|_| "managed_cli_wait_failed")?;
        if !status.success() {
            return Err(match status.code() {
                Some(3) => "managed_delivery_access_denied",
                Some(2) => "managed_delivery_pending",
                _ => "managed_cli_unconfirmed",
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| "managed_cli_invalid_response")
    })
    .await
    .unwrap_or(Err("managed_cli_deadline"));
    if result.is_err() {
        let _ = tokio::time::timeout(Duration::from_secs(2), child.kill()).await;
    }
    result
}

/// One startup pass, capped by 32 records, CLI attempts and a 60-second budget.
/// Mode rollback can leave the journal intact and perform an explicit CLI retry.
pub(crate) async fn replay_pending(rest: &RestClient, dir: &Path) -> Result<usize, &'static str> {
    replay_pending_using(&managed_work::delivery_command(), rest, dir).await
}

pub(crate) async fn replay_pending_using(
    program: &Path,
    rest: &RestClient,
    dir: &Path,
) -> Result<usize, &'static str> {
    tokio::time::timeout(Duration::from_secs(60),async {
        let pending=journal_command(program,rest,dir,"delivery-pending",None).await?;
        let entries=pending["pending"].as_array().filter(|v| v.len()<=32).ok_or("pending_list_invalid")?;
        let mut confirmed=0;
        for entry in entries {
            let key=entry["delivery_key"].as_str().and_then(|s|uuid::Uuid::parse_str(s).ok()).ok_or("pending_key_invalid")?;
            match journal_command(program,rest,dir,"delivery-retry",Some(key)).await {
                Ok(receipt) if receipt["accepted"] == true => { confirmed+=1; tracing::info!(delivery_key=%key,purpose=receipt["purpose"].as_str(),"runtime saved publication confirmed"); }
                _ => tracing::warn!(delivery_key=%key,"runtime saved publication remains pending; original route retained"),
            }
        }
        Ok(confirmed)
    }).await.unwrap_or(Err("runtime_replay_budget_expired"))
}

/// An owner control revokes persisted pending publications independently of
/// worker/work-token lifetime, before that control is acknowledged locally.
pub(crate) async fn revoke_scope(
    rest: &RestClient,
    dir: &Path,
    scope: &crate::scope::SessionScope,
) -> Result<bool, &'static str> {
    let receipt = journal_command_in_scope(
        &managed_work::delivery_command(),
        rest,
        dir,
        "delivery-revoke-scope",
        None,
        Some(scope),
    )
    .await?;
    receipt["revoked"]
        .as_u64()
        .filter(|count| *count <= 1024)
        .map(|count| count > 0)
        .ok_or("runtime_scope_revocation_invalid")
}
