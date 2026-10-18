use super::parse_skill_frontmatter;

#[test]
fn skill_yaml_preserves_words_tags_aliases_and_multiline_text() {
    let source = "---\nname: no\ndescription: |\n  Привет\n  world\nextra: !custom {nested: &a [1, null, true]}\ncopy: *a\n---\nbody";
    assert_eq!(
        parse_skill_frontmatter(source),
        Some(("no".into(), "Привет\nworld".into()))
    );
    for invalid in [
        "---\nname: [unclosed\n---\n",
        "---\nname: *missing\n---\n",
        "---\nname: 123\n---\n",
    ] {
        assert_eq!(parse_skill_frontmatter(invalid), None);
    }
}
