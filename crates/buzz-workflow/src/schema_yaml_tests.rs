use super::*;

#[test]
fn workflow_yaml_preserves_trigger_aliases_and_message_text() {
    let source = "name: no\ntrigger:\n  on: message_posted\nsteps:\n  - id: first\n    action: send_message\n    text: &message |\n      Привет\n      world\n  - id: second\n    action: send_message\n    text: *message\nenabled: false\n";
    let (definition, json) = parse_yaml(source).unwrap();
    assert_eq!(definition.name, "no");
    assert!(!definition.enabled);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["trigger"]["on"], "message_posted");
    assert_eq!(value["steps"][0]["text"], "Привет\nworld\n");
    assert_eq!(value["steps"][0]["text"], value["steps"][1]["text"]);
    let emitted = yaml_serde::to_string(&definition).unwrap();
    let (_, roundtrip) = parse_yaml(&emitted).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&roundtrip).unwrap(),
        value
    );
}

#[test]
fn workflow_yaml_rejects_malformed_duplicate_and_unknown_trigger() {
    for source in [
        "name: [unclosed",
        "name: one\nname: two",
        "name: bad\ntrigger: {on: unknown}\nsteps: []",
        "name: bad\ntrigger: *missing\nsteps: []",
    ] {
        assert!(
            matches!(parse_yaml(source), Err(WorkflowError::InvalidYaml(_))),
            "{source}"
        );
    }
}
