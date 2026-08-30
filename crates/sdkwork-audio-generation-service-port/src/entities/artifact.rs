//! Audio artifact entity (service port shared type)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Artifact kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum ArtifactKind {
    Audio,
    Text,
    Subtitle,
    Waveform,
    Metadata,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::Audio => "audio",
            ArtifactKind::Text => "text",
            ArtifactKind::Subtitle => "subtitle",
            ArtifactKind::Waveform => "waveform",
            ArtifactKind::Metadata => "metadata",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "audio" => Some(ArtifactKind::Audio),
            "text" => Some(ArtifactKind::Text),
            "subtitle" => Some(ArtifactKind::Subtitle),
            "waveform" => Some(ArtifactKind::Waveform),
            "metadata" => Some(ArtifactKind::Metadata),
            _ => None,
        }
    }
}

/// Media kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum MediaKind {
    Speech,
    Transcription,
    Translation,
    SoundEffect,
    Music,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Speech => "speech",
            MediaKind::Transcription => "transcription",
            MediaKind::Translation => "translation",
            MediaKind::SoundEffect => "sound_effect",
            MediaKind::Music => "music",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "speech" => Some(MediaKind::Speech),
            "transcription" => Some(MediaKind::Transcription),
            "translation" => Some(MediaKind::Translation),
            "sound_effect" => Some(MediaKind::SoundEffect),
            "music" => Some(MediaKind::Music),
            _ => None,
        }
    }
}

/// Audio artifact entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AudioArtifact {
    pub id: i64,
    pub artifact_no: String,
    pub task_id: Option<i64>,
    pub request_id: Option<String>,
    pub kind: String,
    pub artifact_type: Option<String>,
    pub title: Option<String>,
    pub voice_id: Option<String>,
    pub provider_code: Option<String>,
    pub provider_asset_id: Option<String>,
    pub artifact_index: i32,
    pub format: Option<String>,
    pub mime_type: Option<String>,
    pub duration_seconds: Option<i32>,
    pub checksum_json: Option<String>,
    pub transcript_text: Option<String>,
    pub translation_text: Option<String>,
    pub media_resource_json: String,
    pub resource_snapshot_json: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub version: i64,
}

/// Create artifact request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateArtifactRequest {
    pub task_id: Option<i64>,
    pub request_id: Option<String>,
    pub kind: ArtifactKind,
    pub artifact_type: Option<String>,
    pub title: Option<String>,
    pub voice_id: Option<String>,
    pub provider_code: Option<String>,
    pub provider_asset_id: Option<String>,
    pub artifact_index: i32,
    pub format: Option<String>,
    pub mime_type: Option<String>,
    pub duration_seconds: Option<i32>,
    pub transcript_text: Option<String>,
    pub translation_text: Option<String>,
    pub media_resource_json: String,
}

/// Artifact filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactFilter {
    pub task_id: Option<i64>,
    pub kind: Option<ArtifactKind>,
    pub media_kind: Option<MediaKind>,
    pub status: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

/// Artifact list result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactListResult {
    pub artifacts: Vec<AudioArtifact>,
    pub total: i64,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}