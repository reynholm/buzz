use super::parse_goose_config;

#[test]
fn goose_yaml_preserves_nested_providers_and_ignores_unknown_tagged_fields() {
    let yaml = "active_provider: custom\nproviders:\n  custom:\n    model: no\n    host: https://example.invalid\nGOOSE_MAX_TOKENS: 001\nextensions:\n  tool:\n    type: stdio\n    enabled: false\nunknown: !custom {nested: [null, true, 2]}\n";
    let config = parse_goose_config(yaml).unwrap();
    assert_eq!(config.provider.as_deref(), Some("custom"));
    assert_eq!(config.model.as_deref(), Some("no"));
    assert_eq!(config.max_output_tokens.as_deref(), Some("001"));
    assert_eq!(
        config.extra.get("custom.host").map(String::as_str),
        Some("https://example.invalid")
    );
    assert_eq!(config.extensions.len(), 1);
    assert!(!config.extensions[0].enabled);
    assert!(parse_goose_config("GOOSE_MODEL: [unclosed").is_none());
}
