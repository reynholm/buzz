//! Upload metadata, file policy, and message attachment formatting.
use crate::error::CliError;

const MAX_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
pub(crate) const MAX_VIDEO_BYTES: u64 = 500 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 100 * 1024 * 1024;

// Mirrors the relay's generic-file deny list. Unknown signatures (including
// Markdown and other text) remain opaque downloads, never inline media.
const BLOCKED_MIMES: &[&str] = &[
    "application/xhtml+xml",
    "image/svg+xml",
    "application/javascript",
    "text/javascript",
    "application/x-msdownload",
    "application/x-executable",
    "application/vnd.microsoft.portable-executable",
    "application/x-mach-binary",
    "application/x-sharedlib",
    "application/x-elf",
    "application/x-msi",
    "application/vnd.android.package-archive",
    "application/x-apple-diskimage",
];

pub(crate) fn validate_upload(mime: &str, size: u64) -> Result<(), CliError> {
    let max = match mime {
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" => MAX_IMAGE_BYTES,
        "video/mp4" => MAX_VIDEO_BYTES,
        _ if BLOCKED_MIMES.contains(&mime)
            || mime.starts_with("image/")
            || mime.starts_with("video/")
            || mime.starts_with("audio/") =>
        {
            return Err(CliError::Usage(format!("unsupported file type: {mime}")));
        }
        _ => MAX_FILE_BYTES,
    };
    if size > max {
        return Err(CliError::Usage(format!(
            "file too large: {size} bytes (max {max})"
        )));
    }
    Ok(())
}

pub(crate) fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(255).collect();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

pub(crate) fn attachment_markdown(desc: &BlobDescriptor) -> String {
    if desc.mime_type.starts_with("video/") {
        return format!("\n![video]({})", desc.url);
    }
    if desc.mime_type.starts_with("image/") {
        return format!("\n![image]({})", desc.url);
    }
    let label = desc.filename.as_deref().unwrap_or("file");
    let escaped = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    format!("\n[{escaped}]({})", desc.url)
}

/// Descriptor returned by the relay after a successful upload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlobDescriptor {
    /// Public URL of the uploaded blob.
    pub url: String,
    /// Hex-encoded SHA-256 of the file content.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME type (e.g. `image/jpeg`).
    #[serde(rename = "type")]
    pub mime_type: String,
    /// Unix timestamp when the file was uploaded.
    pub uploaded: i64,
    /// Image dimensions as `<width>x<height>` (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim: Option<String>,
    /// Blurhash placeholder string (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
    /// Thumbnail URL (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    /// Duration in seconds for video/audio (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// Original basename, retained client-side for download labels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Build an `imeta` tag array from a BlobDescriptor (NIP-92 media metadata).
pub fn build_imeta_tag(d: &BlobDescriptor) -> Vec<String> {
    let mut tag = vec![
        "imeta".to_string(),
        format!("url {}", d.url),
        format!("m {}", d.mime_type),
        format!("x {}", d.sha256),
        format!("size {}", d.size),
    ];
    if let Some(ref dim) = d.dim {
        tag.push(format!("dim {dim}"));
    }
    if let Some(ref bh) = d.blurhash {
        tag.push(format!("blurhash {bh}"));
    }
    if let Some(ref th) = d.thumb {
        tag.push(format!("thumb {th}"));
    }
    if let Some(dur) = d.duration {
        tag.push(format!("duration {dur}"));
    }
    if let Some(ref filename) = d.filename {
        tag.push(format!("filename {filename}"));
    }
    tag
}
