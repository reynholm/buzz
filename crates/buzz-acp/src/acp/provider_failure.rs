//! Translate negotiated Air v1 terminal failures into ordinary ACP errors.
use super::AcpError;
use serde::Deserialize;

#[derive(Deserialize)]
struct Failure {
    category: String,
    severity: String,
    title: String,
    actions: Vec<String>,
}

pub(super) fn check_response(result: &serde_json::Value) -> Result<(), AcpError> {
    let Some(air) = result.pointer("/_meta/jetbrains/air") else {
        return Ok(());
    };
    let Some(failure) = air.get("sessionFailure") else {
        return Ok(());
    };
    if air.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err(AcpError::Protocol(
            "unsupported session failure metadata version".into(),
        ));
    }
    let failure: Failure = serde_json::from_value(failure.clone())
        .map_err(|_| AcpError::Protocol("malformed session failure metadata".into()))?;
    match failure.severity.as_str() {
        "warning" => Ok(()),
        "error" if !failure.title.trim().is_empty() => {
            let auth_required = failure.category == "access"
                && failure.actions.iter().any(|action| action == "login");
            // Preserve useful provider context without allowing a large wire
            // payload to multiply into queue logs and retry-exhaustion notices.
            let title: String = failure.title.trim().chars().take(2048).collect();
            Err(AcpError::AgentError {
                code: if auth_required { -32000 } else { -32603 },
                message: if auth_required {
                    format!("Re-authenticate: {title}")
                } else {
                    title
                },
            })
        }
        _ => Err(AcpError::Protocol(
            "invalid session failure severity or title".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(failure: serde_json::Value) -> serde_json::Value {
        json!({"_meta": {"jetbrains": {"air": {"version": 1, "sessionFailure": failure}}}})
    }

    #[test]
    fn provider_failure_metadata_preserves_error_and_auth_class_on_every_platform() {
        for (category, actions, auth) in [
            ("connection", vec!["retry", "new_session"], false),
            ("service", vec!["retry"], false),
            ("access", vec!["login"], true),
        ] {
            let error = check_response(&response(json!({
                "category": category, "actions": actions,
                "severity": "error", "title": "connection interrupted"
            })))
            .unwrap_err();
            assert!(matches!(error, AcpError::AgentError { .. }));
            assert_eq!(crate::is_auth_error(&error), auth);
        }
    }

    #[test]
    fn provider_failure_metadata_bounds_unicode_diagnostics() {
        let error = check_response(&response(json!({
            "category": "service", "actions": ["retry"],
            "severity": "error", "title": "\u{1f41d}".repeat(4096)
        })))
        .unwrap_err();
        let AcpError::AgentError { message, .. } = error else {
            panic!("expected provider error");
        };
        assert_eq!(message.chars().count(), 2048);
    }
}
