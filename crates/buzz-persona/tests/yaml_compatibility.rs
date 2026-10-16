use buzz_persona::persona::{parse_persona_md, PersonaError};

#[test]
fn pack_validation_preserves_skill_name_warnings_with_unknown_yaml_values() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for path in [".plugin", "agents", "skills/folder"] {
        std::fs::create_dir_all(root.join(path)).unwrap();
    }
    std::fs::write(root.join(".plugin/plugin.json"), r#"{"id":"test.yaml","name":"YAML Pack","version":"0.1.0","personas":["agents/bot.persona.md"]}"#).unwrap();
    std::fs::write(
        root.join("agents/bot.persona.md"),
        "---\nname: bot\ndisplay_name: Bot\ndescription: test\n---\nPrompt\n",
    )
    .unwrap();
    std::fs::write(
        root.join("skills/folder/SKILL.md"),
        "---\nname: no\ndescription: test\nextra: !custom {number: 1, nullish: null}\n---\nSkill\n",
    )
    .unwrap();
    let report = buzz_persona::validate::validate_pack(root);
    assert!(!report.has_errors(), "{report}");
    assert!(
        report
            .to_string()
            .contains("name \"no\" differs from directory name \"folder\""),
        "{report}"
    );
}

#[test]
fn persona_yaml_preserves_scalar_types_aliases_and_multiline_unicode() {
    let source = "---\nname: no\ndisplay_name: &label Соломон\ndescription: |\n  first\n  мир\nmodel: *label\nversion: '001'\nthread_replies: false\ntemperature: 0.25\n---\n# Prompt\n";
    let parsed = parse_persona_md(source).unwrap();
    assert_eq!(parsed.name, "no");
    assert_eq!(parsed.display_name, "Соломон");
    assert_eq!(parsed.model.as_deref(), Some("Соломон"));
    assert_eq!(parsed.description, "first\nмир\n");
    assert_eq!(parsed.version.as_deref(), Some("001"));
    assert_eq!(parsed.thread_replies, Some(false));
    assert_eq!(parsed.temperature, Some(0.25));
    assert_eq!(parsed.prompt, "# Prompt\n");
}

#[test]
fn persona_yaml_rejects_unknown_fields_duplicates_and_bad_aliases() {
    let base = "name: bot\ndisplay_name: Bot\ndescription: test\n";
    for extra in [
        "unknown: true\n",
        "name: other\n",
        "model: *missing\n",
        "model: [unclosed\n",
    ] {
        let error = parse_persona_md(&format!("---\n{base}{extra}---\n")).unwrap_err();
        assert!(matches!(error, PersonaError::Yaml(_)), "{extra}: {error}");
    }
    let source = format!("---\n{base}---\nname: other\n---\n");
    assert_eq!(
        parse_persona_md(&source).unwrap().prompt,
        "name: other\n---\n"
    );
}
