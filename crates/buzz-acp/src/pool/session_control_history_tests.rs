use super::*;
#[test]
fn human_target_reserved_syntax_is_filtered_but_not_executed() {
    for content in [
        "!restart",
        "!rotate",
        "!restart none",
        "!restart all",
        "!compact",
    ] {
        let event = serde_json::json!({"kind":9,"content":content,"tags":[["p","a".repeat(64)]],"id":"b".repeat(64)});
        assert!(
            json_to_context_message(&event).is_none(),
            "reserved control leaked: {content}"
        );
    }
    for content in [
        "!restart foo",
        "!restart recent",
        "@Name !restart",
        "> !restart",
    ] {
        let event = serde_json::json!({"kind":9,"content":content,"tags":[["p","a".repeat(64)]]});
        assert!(json_to_context_message(&event).is_some());
    }
    assert!(
        json_to_context_message(&serde_json::json!({"kind":9,"content":"!restart","tags":[]}))
            .is_some()
    );
}
