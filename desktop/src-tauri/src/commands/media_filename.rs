/// Sanitize a filename for use as a display label in the imeta `filename` field.
///
/// Strips directory components, removes control characters, and bounds length
/// to 255 UTF-8 bytes while preserving the extension for relay ingest validation.
pub(crate) fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    let mut cleaned: String = base.chars().filter(|c| !c.is_control()).collect();
    if cleaned.is_empty() {
        return "file".into();
    }
    if cleaned.len() > 255 {
        // Relay imeta validation bounds UTF-8 bytes, not Unicode characters.
        let suffix = cleaned
            .rsplit_once('.')
            .filter(|(stem, ext)| !stem.is_empty() && ext.len() < 254)
            .map(|(_, ext)| format!(".{ext}"))
            .unwrap_or_default();
        let mut end = 255 - suffix.len();
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        cleaned.truncate(end);
        cleaned.push_str(&suffix);
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    #[test]
    fn strips_paths_controls_and_empty_names() {
        assert_eq!(sanitize_filename("report.pdf"), "report.pdf");
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("/abs/path/notes.txt"), "notes.txt");
        assert_eq!(sanitize_filename(r"C:\Users\me\doc.docx"), "doc.docx");
        assert_eq!(sanitize_filename(""), "file");
        assert_eq!(sanitize_filename("/"), "file");
        assert_eq!(sanitize_filename("a\nb\tc.txt"), "abc.txt");
    }

    #[test]
    fn unicode_labels_fit_relay_utf8_byte_limit_without_losing_extensions() {
        assert_eq!(
            sanitize_filename(&format!("{}.md", "é".repeat(128))),
            format!("{}.md", "é".repeat(126)),
        );
        assert_eq!(
            sanitize_filename(&format!("{}.png", "🐝".repeat(128))),
            format!("{}.png", "🐝".repeat(62)),
        );
    }
}
