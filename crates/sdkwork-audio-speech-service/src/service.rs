//! Speech synthesis service implementation

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn, error};
use uuid::Uuid;

use crate::error::{SpeechServiceError, SpeechServiceResult};
use crate::models::*;
use sdkwork_audio_generation_service_port::entities::task::*;
use sdkwork_audio_generation_service_port::entities::artifact::*;
use sdkwork_audio_generation_service_port::entities::voice::*;
use sdkwork_audio_generation_service_port::repositories::task::TaskRepository;
use sdkwork_audio_generation_service_port::repositories::artifact::ArtifactRepository;
use sdkwork_audio_generation_service_port::repositories::voice::VoiceRepository;
use sdkwork_audio_ai_engine_rust::{AudioAiEngine, SpeechSynthesisRequest as AiSpeechRequest};
use sdkwork_audio_artifact_drive_service::AudioArtifactDriveService;

/// Speech synthesis service trait
#[async_trait]
pub trait SpeechService {
    /// Create a speech synthesis task
    async fn create_speech(
        &self,
        request: SpeechSynthesisRequest,
    ) -> SpeechServiceResult<SpeechSynthesisResponse>;

    /// Get speech synthesis result
    async fn get_speech_result(
        &self,
        task_no: &str,
    ) -> SpeechServiceResult<Option<SpeechSynthesisResult>>;

    /// List available voices
    async fn list_voices(
        &self,
        request: VoiceListRequest,
    ) -> SpeechServiceResult<VoiceListResponse>;

    /// Cancel a speech synthesis task
    async fn cancel_speech(
        &self,
        task_no: &str,
    ) -> SpeechServiceResult<()>;
}

/// Speech service implementation
pub struct SpeechServiceImpl {
    task_repo: Arc<dyn TaskRepository + Send + Sync>,
    artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
    voice_repo: Arc<dyn VoiceRepository + Send + Sync>,
    ai_engine: Arc<dyn AudioAiEngine + Send + Sync>,
    drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
}

impl SpeechServiceImpl {
    pub fn new(
        task_repo: Arc<dyn TaskRepository + Send + Sync>,
        artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
        voice_repo: Arc<dyn VoiceRepository + Send + Sync>,
        ai_engine: Arc<dyn AudioAiEngine + Send + Sync>,
        drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
    ) -> Self {
        Self {
            task_repo,
            artifact_repo,
            voice_repo,
            ai_engine,
            drive_service,
        }
    }

    /// Calculate input hash for idempotency
    pub fn calculate_input_hash(request: &SpeechSynthesisRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(request.text.as_bytes());
        hasher.update(request.text_format.as_str().as_bytes());
        if let Some(ref language) = request.language {
            hasher.update(language.as_bytes());
        }
        if let Some(ref voice_id) = request.voice_id {
            hasher.update(voice_id.as_bytes());
        }
        hasher.update(request.speed.to_string().as_bytes());
        hasher.update(request.pitch.to_string().as_bytes());
        hasher.update(request.volume.to_string().as_bytes());
        hasher.update(request.audio_format.as_str().as_bytes());
        hasher.update(request.sample_rate.to_string().as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Process speech synthesis task
    async fn process_speech_task(
        &self,
        task: &AudioGenerationTask,
        request: &SpeechSynthesisRequest,
    ) -> SpeechServiceResult<()> {
        info!("Processing speech task: {}", task.task_no);

        // Update task status to running
        self.task_repo.update(task.id, UpdateTaskRequest {
            status: Some(TaskStatus::Running),
            progress: Some(10),
            ..Default::default()
        }).await?;

        // Create AI engine request
        let ai_request = AiSpeechRequest {
            text: request.text.clone(),
            text_format: request.text_format.as_str().to_string(),
            language: request.language.clone(),
            voice_id: request.voice_id.clone(),
            speed: request.speed,
            pitch: request.pitch,
            volume: request.volume,
            emotion: request.emotion.clone(),
            emotion_intensity: request.emotion_intensity,
            audio_format: request.audio_format.as_str().to_string(),
            sample_rate: request.sample_rate,
        };

        // Call AI engine
        let ai_result = self.ai_engine.synthesize_speech(ai_request).await
            .map_err(|e| SpeechServiceError::AiEngine(e.to_string()))?;

        // Update progress
        self.task_repo.update(task.id, UpdateTaskRequest {
            progress: Some(50),
            ..Default::default()
        }).await?;

        // Upload to drive
        let filename = format!("speech_{}.{}", task.task_no, request.audio_format.as_str());
        let drive_result = self.drive_service.upload_artifact(
            task.tenant_id,
            task.user_id,
            &ai_result.audio_data,
            ai_result.mime_type.as_str(),
            &filename,
        ).await.map_err(|e| SpeechServiceError::DriveService(e.to_string()))?;

        // Update progress
        self.task_repo.update(task.id, UpdateTaskRequest {
            progress: Some(80),
            ..Default::default()
        }).await?;

        // Create artifact
        let media_resource = serde_json::json!({
            "source": "drive",
            "driveSpaceId": drive_result.drive_space_id,
            "driveNodeId": drive_result.drive_node_id,
            "driveResourceUri": drive_result.drive_resource_uri,
        });

        self.artifact_repo.create(CreateArtifactRequest {
            task_id: Some(task.id),
            request_id: None,
            kind: ArtifactKind::Audio,
            artifact_type: Some("speech".to_string()),
            title: Some(format!("Speech synthesis: {}", &request.text[..std::cmp::min(50, request.text.len())])),
            voice_id: request.voice_id.clone(),
            provider_code: Some(self.ai_engine.engine_type().as_str().to_string()),
            provider_asset_id: None,
            artifact_index: 0,
            format: Some(request.audio_format.as_str().to_string()),
            mime_type: Some(ai_result.mime_type.clone()),
            duration_seconds: Some((ai_result.duration_ms / 1000) as i32),
            transcript_text: None,
            translation_text: None,
            media_resource_json: media_resource.to_string(),
        }).await?;

        // Update task to completed
        let result = serde_json::json!({
            "durationMs": ai_result.duration_ms,
            "sampleRate": ai_result.sample_rate,
            "channels": ai_result.channels,
            "fileSizeBytes": ai_result.audio_data.len(),
            "driveSpaceId": drive_result.drive_space_id,
            "driveNodeId": drive_result.drive_node_id,
        });

        self.task_repo.update(task.id, UpdateTaskRequest {
            status: Some(TaskStatus::Succeeded),
            progress: Some(100),
            result_json: Some(result.to_string()),
            completed_at: Some(chrono::Utc::now()),
            ..Default::default()
        }).await?;

        info!("Speech task completed: {}", task.task_no);
        Ok(())
    }
}

#[async_trait]
impl SpeechService for SpeechServiceImpl {
    async fn create_speech(
        &self,
        request: SpeechSynthesisRequest,
    ) -> SpeechServiceResult<SpeechSynthesisResponse> {
        info!("Creating speech synthesis task for user: {}", request.user_id);

        // Check idempotency
        if let Some(ref idempotency_key) = request.idempotency_key {
            let existing = self.task_repo.get_by_idempotency_key(
                request.tenant_id,
                OperationType::Speech.as_str(),
                idempotency_key,
            ).await?;

            if let Some(task) = existing {
                warn!("Task already exists with idempotency key: {}", idempotency_key);
                return Ok(SpeechSynthesisResponse {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    estimated_duration_ms: None,
                });
            }
        }

        // Calculate input hash
        let input_hash = Self::calculate_input_hash(&request);

        // Create task
        let request_json = serde_json::to_string(&request)
            .map_err(|e| SpeechServiceError::Internal(e.to_string()))?;

        let task = self.task_repo.create(CreateTaskRequest {
            tenant_id: request.tenant_id,
            organization_id: request.organization_id,
            user_id: request.user_id,
            operation_type: OperationType::Speech,
            provider_code: self.ai_engine.engine_type().as_str().to_string(),
            provider_route_id: None,
            model: None,
            idempotency_key: request.idempotency_key.clone(),
            request_json: request_json.clone(),
            callback_url: None,
        }).await?;

        // Process task asynchronously
        let task_clone = task.clone();
        let request_clone = request.clone();
        let self_clone = Self {
            task_repo: self.task_repo.clone(),
            artifact_repo: self.artifact_repo.clone(),
            voice_repo: self.voice_repo.clone(),
            ai_engine: self.ai_engine.clone(),
            drive_service: self.drive_service.clone(),
        };

        tokio::spawn(async move {
            if let Err(e) = self_clone.process_speech_task(&task_clone, &request_clone).await {
                error!("Failed to process speech task: {}", e);
                // Update task to failed
                let _ = self_clone.task_repo.update(task_clone.id, UpdateTaskRequest {
                    status: Some(TaskStatus::Failed),
                    error_code: Some("PROCESSING_ERROR".to_string()),
                    error_message: Some(e.to_string()),
                    completed_at: Some(chrono::Utc::now()),
                    ..Default::default()
                }).await;
            }
        });

        Ok(SpeechSynthesisResponse {
            task_id: task.id.to_string(),
            task_no: task.task_no,
            status: task.status,
            estimated_duration_ms: Some(2000), // Estimated 2 seconds
        })
    }

    async fn get_speech_result(
        &self,
        task_no: &str,
    ) -> SpeechServiceResult<Option<SpeechSynthesisResult>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                let result = if let Some(ref result_json) = task.result_json {
                    serde_json::from_str(result_json).ok()
                } else {
                    None
                };

                Ok(Some(SpeechSynthesisResult {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    audio_url: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("driveResourceUri").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    duration_ms: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("durationMs").and_then(|v| v.as_u64())
                    }),
                    file_size_bytes: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("fileSizeBytes").and_then(|v| v.as_u64())
                    }),
                    mime_type: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("mimeType").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                }))
            }
            None => Ok(None),
        }
    }

    async fn list_voices(
        &self,
        request: VoiceListRequest,
    ) -> SpeechServiceResult<VoiceListResponse> {
        let filter = VoiceFilter {
            tenant_id: Some(request.tenant_id),
            user_id: request.user_id,
            language: request.language,
            gender: request.gender,
            voice_type: request.voice_type.and_then(|vt| VoiceType::from_str(&vt)),
            status: Some(VoiceStatus::Active),
            is_public: None,
            created_after: None,
            created_before: None,
        };

        let result = self.voice_repo.list(filter, request.limit, request.offset).await?;

        let voices = result.voices.into_iter().map(|v| VoiceInfo {
            voice_id: v.id.to_string(),
            voice_no: v.voice_no,
            name: v.name,
            description: v.description,
            language: v.language,
            gender: v.gender,
            voice_type: v.voice_type,
            preview_url: None,
        }).collect();

        Ok(VoiceListResponse {
            voices,
            total: result.total,
            has_more: result.has_more,
            next_cursor: result.next_cursor,
        })
    }

    async fn cancel_speech(
        &self,
        task_no: &str,
    ) -> SpeechServiceResult<()> {
        let task = self.task_repo.get_by_task_no(task_no).await?
            .ok_or_else(|| SpeechServiceError::TaskNotFound {
                task_no: task_no.to_string(),
            })?;

        if task.status == TaskStatus::Succeeded.as_str() || task.status == TaskStatus::Failed.as_str() {
            return Err(SpeechServiceError::InvalidRequest {
                message: "Cannot cancel completed task".to_string(),
            });
        }

        self.task_repo.update(task.id, UpdateTaskRequest {
            status: Some(TaskStatus::Cancelled),
            completed_at: Some(chrono::Utc::now()),
            ..Default::default()
        }).await?;

        Ok(())
    }
}

