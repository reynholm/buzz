use super::rewrite_legacy_persona_md_runtime;

#[test]
fn runtime_rewrite_preserves_unknown_yaml_values_and_markdown() {
    let frontmatter = "runtime: sprout-agent\nname: no\nunknown: !custom {number: 42, text: '001', nullish: null}\nnumeric: {1: one, true: enabled}\nbase: &a {word: no}\nalias: *a\nmerged: {<<: *a}\nfloat: .inf\ntext: |\n  Привет\n  world\n";
    let body = "# Prompt\n\n```yaml\nkeep: unchanged\n```\n";
    let source = format!("---\n{frontmatter}---\n{body}");
    let updated = rewrite_legacy_persona_md_runtime(&source).unwrap();
    let (yaml, actual_body) = buzz_persona_pkg::persona::split_frontmatter(&updated).unwrap();
    assert_eq!(actual_body, body);
    let (original_yaml, _) = buzz_persona_pkg::persona::split_frontmatter(&source).unwrap();
    let mut expected: yaml_serde::Value = yaml_serde::from_str(original_yaml).unwrap();
    expected["runtime"] = yaml_serde::Value::String("buzz-agent".into());
    assert_eq!(
        yaml_serde::from_str::<yaml_serde::Value>(yaml).unwrap(),
        expected
    );
    assert!(updated.contains("!custom"));
    assert_eq!(rewrite_legacy_persona_md_runtime(&updated), None);
    assert_eq!(
        rewrite_legacy_persona_md_runtime("---\nruntime: [broken\n---\n"),
        None
    );
    assert_eq!(
        rewrite_legacy_persona_md_runtime("---\nruntime: sprout-agent\nruntime: other\n---\n"),
        None
    );
}
