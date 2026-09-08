use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Video,
    Audio,
    Image,
    Subtitle,
    Json,
    Other,
}

impl ArtifactKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "flv" | "wmv" | "m4v" => Self::Video,
            "mp3" | "wav" | "aac" | "flac" | "ogg" | "m4a" | "wma" => Self::Audio,
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" => Self::Image,
            "srt" | "vtt" | "ass" | "ssa" | "sub" => Self::Subtitle,
            "json" => Self::Json,
            _ => Self::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Subtitle => "subtitle",
            Self::Json => "json",
            Self::Other => "other",
        }
    }
}

impl std::str::FromStr for ArtifactKind {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kind = match s.to_lowercase().as_str() {
            "video" => Self::Video,
            "audio" => Self::Audio,
            "image" => Self::Image,
            "subtitle" => Self::Subtitle,
            "json" => Self::Json,
            _ => Self::Other,
        };
        Ok(kind)
    }
}

pub fn guess_mime_type(ext: &str) -> Option<String> {
    let mime = match ext.to_lowercase().as_str() {
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "srt" => "text/plain",
        "vtt" => "text/vtt",
        "json" => "application/json",
        _ => return None,
    };
    Some(mime.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub kind: ArtifactKind,
    pub uri: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub sha256: String,
    pub kind: ArtifactKind,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub storage_key: String,
    pub original_filename: String,
    pub created_at: String,
}

impl Artifact {
    pub fn to_ref(&self) -> ArtifactRef {
        ArtifactRef {
            artifact_id: self.id.clone(),
            kind: self.kind,
            uri: format!("artifact://{}", self.id),
            sha256: self.sha256.clone(),
            size_bytes: self.size_bytes,
            mime_type: self.mime_type.clone(),
        }
    }
}
