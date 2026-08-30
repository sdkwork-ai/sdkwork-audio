//! Audio voice entity (service port shared type)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Voice type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum VoiceType {
    Prebuilt,
    Custom,
    Cloned,
}

impl VoiceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            VoiceType::Prebuilt => "prebuilt",
            VoiceType::Custom => "custom",
            VoiceType::Cloned => "cloned",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "prebuilt" => Some(VoiceType::Prebuilt),
            "custom" => Some(VoiceType::Custom),
            "cloned" => Some(VoiceType::Cloned),
            _ => None,
        }
    }
}

/// Voice status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum VoiceStatus {
    Active,
    Inactive,
    Deprecated,
}

impl VoiceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VoiceStatus::Active => "active",
            VoiceStatus::Inactive => "inactive",
            VoiceStatus::Deprecated => "deprecated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(VoiceStatus::Active),
            "inactive" => Some(VoiceStatus::Inactive),
            "deprecated" => Some(VoiceStatus::Deprecated),
            _ => None,
        }
    }
}

/// Audio voice entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AudioVoice {
    pub id: i64,
    pub voice_no: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub user_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub language: String,
    pub gender: Option<String>,
    pub voice_type: String,
    pub provider_code: Option<String>,
    pub provider_voice_id: Option<String>,
    pub provider_config_json: Option<String>,
    pub clone_reference_url: Option<String>,
    pub clone_parameters_json: Option<String>,
    pub status: String,
    pub is_public: bool,
    pub usage_count: i64,
    pub quality_score: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub version: i64,
}

/// Create voice request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVoiceRequest {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub user_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub language: String,
    pub gender: Option<String>,
    pub voice_type: VoiceType,
    pub provider_code: Option<String>,
    pub provider_voice_id: Option<String>,
    pub provider_config_json: Option<String>,
    pub clone_reference_url: Option<String>,
    pub clone_parameters_json: Option<String>,
    pub is_public: bool,
}

/// Update voice request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateVoiceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub gender: Option<String>,
    pub voice_type: Option<VoiceType>,
    pub provider_code: Option<String>,
    pub provider_voice_id: Option<String>,
    pub provider_config_json: Option<String>,
    pub clone_reference_url: Option<String>,
    pub clone_parameters_json: Option<String>,
    pub status: Option<VoiceStatus>,
    pub is_public: Option<bool>,
}

/// Voice filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceFilter {
    pub tenant_id: Option<i64>,
    pub user_id: Option<i64>,
    pub language: Option<String>,
    pub gender: Option<String>,
    pub voice_type: Option<VoiceType>,
    pub status: Option<VoiceStatus>,
    pub is_public: Option<bool>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

/// Voice list result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceListResult {
    pub voices: Vec<AudioVoice>,
    pub total: i64,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}