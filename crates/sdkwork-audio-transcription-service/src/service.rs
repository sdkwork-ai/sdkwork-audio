//! Transcription service implementation

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn, error};
use uuid::Uuid;

use crate::error::{TranscriptionServiceError, TranscriptionServiceResult};
use crate::models::*;
use sdkwork_audio_generation_service_port::entities::task::*;
use sdkwork_audio_generation_service_port::entities::artifact::*;
use sdkwork_audio_generation_service_port::repositories::task::TaskRepository;
use sdkwork_audio_generation_service_port::repositories::artifact::ArtifactRepository;
use sdkwork_audio_ai_engine_rust::{AudioAiEngine, TranscriptionRequest as AiTranscriptionRequest};
use sdkwork_audio_artifact_drive_service::AudioArtifactDriveService;

/// Transcription service trait
#[async_trait]
pub trait TranscriptionService {
    /// Create a transcription task
    async fn create_transcription(
        &self,
        request: TranscriptionRequest,
    ) -> TranscriptionServiceResult<TranscriptionResponse>;

    /// Get transcription result
    async fn get_transcription_result(
        &self,
        task_no: &str,
    ) -> TranscriptionServiceResult<Option<TranscriptionResult>>;

    /// Get transcription task detail
    async fn get_transcription_detail(
        &self,
        task_no: &str,
    ) -> TranscriptionServiceResult<Option<TranscriptionTaskDetail>>;

    /// List transcription tasks
    async fn list_transcriptions(
        &self,
        request: TranscriptionListRequest,
    ) -> TranscriptionServiceResult<TranscriptionListResponse>;

    /// Cancel a transcription task
    async fn cancel_transcription(
        &self,
        task_no: &str,
    ) -> TranscriptionServiceResult<()>;
}

/// Transcription service implementation
pub struct TranscriptionServiceImpl {
    task_repo: Arc<dyn TaskRepository + Send + Sync>,
    artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
    ai_engine: Arc<dyn AudioAiEngine + Send + Sync>,
    drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
}

impl TranscriptionServiceImpl {
    pub fn new(
        task_repo: Arc<dyn TaskRepository + Send + Sync>,
        artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
        ai_engine: Arc<dyn AudioAiEngine + Send + Sync>,
        drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
    ) -> Self {
        Self {
            task_repo,
            artifact_repo,
            ai_engine,
            drive_service,
        }
    }

    /// Calculate input hash for idempotency
    fn calculate_input_hash(request: &TranscriptionRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(request.audio_url.as_bytes());
        if let Some(ref language) = request.language {
            hasher.update(language.as_bytes());
        }
        hasher.update(request.detect_language.to_string().as_bytes());
        hasher.update(request.enable_timestamps.to_string().as_bytes());
        hasher.update(request.enable_speaker_diarization.to_string().as_bytes());
        hasher.update(request.output_format.as_str().as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Process transcription task
    async fn process_transcription_task(
        &self,
        task: &AudioGenerationTask,
        request: &TranscriptionRequest,
    ) -> TranscriptionServiceResult<()> {
        info!("Processing transcription task: {}", task.task_no);

        // Update task status to running
        self.task_repo.update(task.id, UpdateTaskRequest {
            status: Some(TaskStatus::Running),
            progress: Some(10),
            ..Default::default()
        }).await?;

        // Create AI engine request
        let ai_request = AiTranscriptionRequest {
            audio_url: request.audio_url.clone(),
            language: request.language.clone(),
            detect_language: request.detect_language,
            enable_timestamps: request.enable_timestamps,
            enable_speaker_diarization: request.enable_speaker_diarization,
            max_speakers: request.max_speakers,
            output_format: request.output_format.as_str().to_string(),
        };

        // Update progress
        self.task_repo.update(task.id, UpdateTaskRequest {
            progress: Some(30),
            ..Default::default()
        }).await?;

        // Call AI engine
        let ai_result = self.ai_engine.transcribe_audio(ai_request).await
            .map_err(|e| TranscriptionServiceError::AiEngine(e.to_string()))?;

        // Update progress
        self.task_repo.update(task.id, UpdateTaskRequest {
            progress: Some(60),
            ..Default::default()
        }).await?;

        // Create output content based on format
        let output_content = match request.output_format {
            OutputFormat::JSON => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "text": ai_result.text,
                    "language": ai_result.language,
                    "confidence": ai_result.confidence,
                    "segments": ai_result.segments.iter().map(|s| {
                        serde_json::json!({
                            "startMs": s.start_ms,
                            "endMs": s.end_ms,
                            "text": s.text,
                            "speakerId": s.speaker_id,
                            "confidence": s.confidence,
                        })
                    }).collect::<Vec<_>>(),
                })).unwrap_or_default()
            }
            OutputFormat::SRT => {
                let mut srt = String::new();
                for (i, segment) in ai_result.segments.iter().enumerate() {
                    srt.push_str(&format!("{}\n", i + 1));
                    srt.push_str(&format!("{} --> {}\n",
                        Self::format_srt_time(segment.start_ms),
                        Self::format_srt_time(segment.end_ms)
                    ));
                    srt.push_str(&format!("{}\n\n", segment.text));
                }
                srt
            }
            OutputFormat::VTT => {
                let mut vtt = String::from("WEBVTT\n\n");
                for segment in &ai_result.segments {
                    vtt.push_str(&format!("{} --> {}\n",
                        Self::format_vtt_time(segment.start_ms),
                        Self::format_vtt_time(segment.end_ms)
                    ));
                    vtt.push_str(&format!("{}\n\n", segment.text));
                }
                vtt
            }
            OutputFormat::TXT => {
                ai_result.text.clone()
            }
        };

        // Upload output to drive
        let filename = format!("transcription_{}.{}", task.task_no, request.output_format.as_str());
        let mime_type = match request.output_format {
            OutputFormat::JSON => "application/json",
            OutputFormat::SRT => "text/srt",
            OutputFormat::VTT => "text/vtt",
            OutputFormat::TXT => "text/plain",
        };

        let drive_result = self.drive_service.upload_artifact(
            task.tenant_id,
            task.user_id,
            output_content.as_bytes(),
            mime_type,
            &filename,
        ).await.map_err(|e| TranscriptionServiceError::DriveService(e.to_string()))?;

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
            kind: ArtifactKind::Text,
            artifact_type: Some("transcription".to_string()),
            title: Some(format!("Transcription: {}", &request.audio_url[..std::cmp::min(50, request.audio_url.len())])),
            voice_id: None,
            provider_code: Some(self.ai_engine.engine_type().as_str().to_string()),
            provider_asset_id: None,
            artifact_index: 0,
            format: Some(request.output_format.as_str().to_string()),
            mime_type: Some(mime_type.to_string()),
            duration_seconds: None,
            transcript_text: Some(ai_result.text.clone()),
            translation_text: None,
            media_resource_json: media_resource.to_string(),
        }).await?;

        // Update task to completed
        let segments_json = serde_json::to_value(&ai_result.segments).unwrap_or_default();
        let result = serde_json::json!({
            "text": ai_result.text,
            "language": ai_result.language,
            "confidence": ai_result.confidence,
            "segments": segments_json,
            "driveSpaceId": drive_result.drive_space_id,
            "driveNodeId": drive_result.drive_node_id,
            "outputUrl": drive_result.drive_resource_uri,
        });

        self.task_repo.update(task.id, UpdateTaskRequest {
            status: Some(TaskStatus::Succeeded),
            progress: Some(100),
            result_json: Some(result.to_string()),
            completed_at: Some(chrono::Utc::now()),
            ..Default::default()
        }).await?;

        info!("Transcription task completed: {}", task.task_no);
        Ok(())
    }

    /// Format milliseconds to SRT time format (HH:MM:SS,mmm)
    fn format_srt_time(ms: u64) -> String {
        let hours = ms / 3600000;
        let minutes = (ms % 3600000) / 60000;
        let seconds = (ms % 60000) / 1000;
        let milliseconds = ms % 1000;
        format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds, milliseconds)
    }

    /// Format milliseconds to VTT time format (HH:MM:SS.mmm)
    fn format_vtt_time(ms: u64) -> String {
        let hours = ms / 3600000;
        let minutes = (ms % 3600000) / 60000;
        let seconds = (ms % 60000) / 1000;
        let milliseconds = ms % 1000;
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, milliseconds)
    }
}

#[async_trait]
impl TranscriptionService for TranscriptionServiceImpl {
    async fn create_transcription(
        &self,
        request: TranscriptionRequest,
    ) -> TranscriptionServiceResult<TranscriptionResponse> {
        info!("Creating transcription task for user: {}", request.user_id);

        // Check idempotency
        if let Some(ref idempotency_key) = request.idempotency_key {
            let existing = self.task_repo.get_by_idempotency_key(
                request.tenant_id,
                OperationType::Transcription.as_str(),
                idempotency_key,
            ).await?;

            if let Some(task) = existing {
                warn!("Task already exists with idempotency key: {}", idempotency_key);
                return Ok(TranscriptionResponse {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    estimated_duration_ms: None,
                });
            }
        }

        // Validate request
        if request.audio_url.is_empty() {
            return Err(TranscriptionServiceError::InvalidRequest {
                message: "Audio URL is required".to_string(),
            });
        }

        // Calculate input hash
        let input_hash = Self::calculate_input_hash(&request);

        // Create task
        let request_json = serde_json::to_string(&request)
            .map_err(|e| TranscriptionServiceError::Internal(e.to_string()))?;

        let task = self.task_repo.create(CreateTaskRequest {
            tenant_id: request.tenant_id,
            organization_id: request.organization_id,
            user_id: request.user_id,
            operation_type: OperationType::Transcription,
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
            ai_engine: self.ai_engine.clone(),
            drive_service: self.drive_service.clone(),
        };

        tokio::spawn(async move {
            if let Err(e) = self_clone.process_transcription_task(&task_clone, &request_clone).await {
                error!("Failed to process transcription task: {}", e);
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

        Ok(TranscriptionResponse {
            task_id: task.id.to_string(),
            task_no: task.task_no,
            status: task.status,
            estimated_duration_ms: Some(5000), // Estimated 5 seconds
        })
    }

    async fn get_transcription_result(
        &self,
        task_no: &str,
    ) -> TranscriptionServiceResult<Option<TranscriptionResult>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                if task.operation_type != OperationType::Transcription.as_str() {
                    return Err(TranscriptionServiceError::InvalidRequest {
                        message: "Task is not a transcription task".to_string(),
                    });
                }

                let result = if let Some(ref result_json) = task.result_json {
                    serde_json::from_str(result_json).ok()
                } else {
                    None
                };

                Ok(Some(TranscriptionResult {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    text: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    language: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("language").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    segments: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("segments").and_then(|v| serde_json::from_value(v.clone()).ok())
                    }),
                    confidence: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("confidence").and_then(|v| v.as_f64())
                    }),
                    duration_ms: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("durationMs").and_then(|v| v.as_u64())
                    }),
                    output_url: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("outputUrl").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_transcription_detail(
        &self,
        task_no: &str,
    ) -> TranscriptionServiceResult<Option<TranscriptionTaskDetail>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                if task.operation_type != OperationType::Transcription.as_str() {
                    return Err(TranscriptionServiceError::InvalidRequest {
                        message: "Task is not a transcription task".to_string(),
                    });
                }

                let request_params: TranscriptionRequest = serde_json::from_str(&task.request_json)
                    .unwrap_or_else(|_| TranscriptionRequest {
                        tenant_id: task.tenant_id,
                        organization_id: task.organization_id,
                        user_id: task.user_id,
                        audio_url: String::new(),
                        language: None,
                        detect_language: true,
                        enable_timestamps: true,
                        enable_speaker_diarization: false,
                        max_speakers: None,
                        output_format: OutputFormat::JSON,
                        include_confidence: true,
                        include_words: true,
                        idempotency_key: None,
                    });

                let result = self.get_transcription_result(task_no).await?;

                Ok(Some(TranscriptionTaskDetail {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    progress: task.progress,
                    request_params,
                    result,
                    created_at: task.created_at.to_rfc3339(),
                    completed_at: task.completed_at.map(|dt| dt.to_rfc3339()),
                }))
            }
            None => Ok(None),
        }
    }

    async fn list_transcriptions(
        &self,
        request: TranscriptionListRequest,
    ) -> TranscriptionServiceResult<TranscriptionListResponse> {
        let filter = TaskFilter {
            tenant_id: Some(request.tenant_id),
            user_id: request.user_id,
            organization_id: None,
            operation_type: Some(OperationType::Transcription),
            status: request.status.and_then(|s| TaskStatus::from_str(&s)),
            provider_code: None,
            created_after: None,
            created_before: None,
        };

        let result = self.task_repo.list(filter, request.limit, request.offset).await?;

        let mut tasks = Vec::new();
        for task in result.tasks {
            let detail = self.get_transcription_detail(&task.task_no).await?;
            if let Some(detail) = detail {
                tasks.push(detail);
            }
        }

        Ok(TranscriptionListResponse {
            tasks,
            total: result.total,
            has_more: result.has_more,
            next_cursor: result.next_cursor,
        })
    }

    async fn cancel_transcription(
        &self,
        task_no: &str,
    ) -> TranscriptionServiceResult<()> {
        let task = self.task_repo.get_by_task_no(task_no).await?
            .ok_or_else(|| TranscriptionServiceError::TaskNotFound {
                task_no: task_no.to_string(),
            })?;

        if task.operation_type != OperationType::Transcription.as_str() {
            return Err(TranscriptionServiceError::InvalidRequest {
                message: "Task is not a transcription task".to_string(),
            });
        }

        if task.status == TaskStatus::Succeeded.as_str() || task.status == TaskStatus::Failed.as_str() {
            return Err(TranscriptionServiceError::InvalidRequest {
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
