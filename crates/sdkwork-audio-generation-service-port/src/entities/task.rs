//! Audio generation task entity (service port shared type)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Task status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Routing,
    Submitted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Expired,
    NeedsReview,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Routing => "routing",
            TaskStatus::Submitted => "submitted",
            TaskStatus::Running => "running",
            TaskStatus::Succeeded => "succeeded",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::Expired => "expired",
            TaskStatus::NeedsReview => "needs_review",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(TaskStatus::Queued),
            "routing" => Some(TaskStatus::Routing),
            "submitted" => Some(TaskStatus::Submitted),
            "running" => Some(TaskStatus::Running),
            "succeeded" => Some(TaskStatus::Succeeded),
            "failed" => Some(TaskStatus::Failed),
            "cancelled" => Some(TaskStatus::Cancelled),
            "expired" => Some(TaskStatus::Expired),
            "needs_review" => Some(TaskStatus::NeedsReview),
            _ => None,
        }
    }
}

/// Operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "snake_case")]
pub enum OperationType {
    Speech,
    Transcription,
    Translation,
    SoundEffect,
    Music,
}

impl OperationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationType::Speech => "speech",
            OperationType::Transcription => "transcription",
            OperationType::Translation => "translation",
            OperationType::SoundEffect => "sound_effect",
            OperationType::Music => "music",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "speech" => Some(OperationType::Speech),
            "transcription" => Some(OperationType::Transcription),
            "translation" => Some(OperationType::Translation),
            "sound_effect" => Some(OperationType::SoundEffect),
            "music" => Some(OperationType::Music),
            _ => None,
        }
    }
}

/// Audio generation task entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AudioGenerationTask {
    pub id: i64,
    pub task_no: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub user_id: i64,
    pub operation_type: String,
    pub provider_code: String,
    pub provider_route_id: Option<i64>,
    pub model: Option<String>,
    pub provider_task_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub input_hash: Option<String>,
    pub status: String,
    pub progress: i32,
    pub request_json: String,
    pub normalized_options_json: Option<String>,
    pub provider_request_json: Option<String>,
    pub provider_response_json: Option<String>,
    pub result_json: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub callback_url: Option<String>,
    pub callback_status: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
    pub version: i64,
}

/// Create task request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub user_id: i64,
    pub operation_type: OperationType,
    pub provider_code: String,
    pub provider_route_id: Option<i64>,
    pub model: Option<String>,
    pub idempotency_key: Option<String>,
    pub request_json: String,
    pub callback_url: Option<String>,
}

/// Update task request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub status: Option<TaskStatus>,
    pub progress: Option<i32>,
    pub provider_code: Option<String>,
    pub provider_route_id: Option<i64>,
    pub provider_task_id: Option<String>,
    pub provider_request_json: Option<String>,
    pub provider_response_json: Option<String>,
    pub result_json: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub callback_status: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl Default for UpdateTaskRequest {
    fn default() -> Self {
        Self {
            status: None,
            progress: None,
            provider_code: None,
            provider_route_id: None,
            provider_task_id: None,
            provider_request_json: None,
            provider_response_json: None,
            result_json: None,
            error_code: None,
            error_message: None,
            callback_status: None,
            submitted_at: None,
            completed_at: None,
            expires_at: None,
        }
    }
}

/// Task filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFilter {
    pub tenant_id: Option<i64>,
    pub user_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub operation_type: Option<OperationType>,
    pub status: Option<TaskStatus>,
    pub provider_code: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

/// Task list result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListResult {
    pub tasks: Vec<AudioGenerationTask>,
    pub total: i64,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}