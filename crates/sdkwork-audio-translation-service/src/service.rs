//! Translation service implementation

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn, error};
use uuid::Uuid;

use crate::error::{TranslationServiceError, TranslationServiceResult};
use crate::models::*;
use sdkwork_audio_generation_service_port::entities::task::*;
use sdkwork_audio_generation_service_port::entities::artifact::*;
use sdkwork_audio_generation_service_port::repositories::task::TaskRepository;
use sdkwork_audio_generation_service_port::repositories::artifact::ArtifactRepository;
use sdkwork_audio_ai_engine_rust::{AudioAiEngine, TranslationRequest as AiTranslationRequest};
use sdkwork_audio_artifact_drive_service::AudioArtifactDriveService;

/// Translation service trait
#[async_trait]
pub trait TranslationService {
    /// Create a translation task
    async fn create_translation(
        &self,
        request: TranslationRequest,
    ) -> TranslationServiceResult<TranslationResponse>;

    /// Get translation result
    async fn get_translation_result(
        &self,
        task_no: &str,
    ) -> TranslationServiceResult<Option<TranslationResult>>;

    /// Get translation task detail
    async fn get_translation_detail(
        &self,
        task_no: &str,
    ) -> TranslationServiceResult<Option<TranslationTaskDetail>>;

    /// List translation tasks
    async fn list_translations(
        &self,
        request: TranslationListRequest,
    ) -> TranslationServiceResult<TranslationListResponse>;

    /// Cancel a translation task
    async fn cancel_translation(
        &self,
        task_no: &str,
    ) -> TranslationServiceResult<()>;

    /// Get supported languages
    async fn get_supported_languages(
        &self,
    ) -> TranslationServiceResult<SupportedLanguagesResponse>;
}

/// Translation service implementation
pub struct TranslationServiceImpl {
    task_repo: Arc<dyn TaskRepository + Send + Sync>,
    artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
    ai_engine: Arc<dyn AudioAiEngine + Send + Sync>,
    drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
}

impl TranslationServiceImpl {
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
    fn calculate_input_hash(request: &TranslationRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(request.audio_url.as_bytes());
        if let Some(ref source_language) = request.source_language {
            hasher.update(source_language.as_bytes());
        }
        hasher.update(request.target_language.as_bytes());
        hasher.update(request.output_format.as_str().as_bytes());
        hasher.update(request.include_timestamps.to_string().as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Process translation task
    async fn process_translation_task(
        &self,
        task: &AudioGenerationTask,
        request: &TranslationRequest,
    ) -> TranslationServiceResult<()> {
        info!("Processing translation task: {}", task.task_no);

        // Update task status to running
        self.task_repo.update(task.id, UpdateTaskRequest {
            status: Some(TaskStatus::Running),
            progress: Some(10),
            ..Default::default()
        }).await?;

        // Create AI engine request
        let ai_request = AiTranslationRequest {
            audio_url: request.audio_url.clone(),
            source_language: request.source_language.clone(),
            target_language: request.target_language.clone(),
            output_format: request.output_format.as_str().to_string(),
        };

        // Update progress
        self.task_repo.update(task.id, UpdateTaskRequest {
            progress: Some(30),
            ..Default::default()
        }).await?;

        // Call AI engine
        let ai_result = self.ai_engine.translate_audio(ai_request).await
            .map_err(|e| TranslationServiceError::AiEngine(e.to_string()))?;

        // Update progress
        self.task_repo.update(task.id, UpdateTaskRequest {
            progress: Some(60),
            ..Default::default()
        }).await?;

        // Create output content based on format
        let output_content = match request.output_format {
            OutputFormat::JSON => {
                serde_json::to_string_pretty(&serde_json::json!({
                    "sourceText": ai_result.source_text,
                    "translatedText": ai_result.translated_text,
                    "sourceLanguage": ai_result.source_language,
                    "targetLanguage": ai_result.target_language,
                    "confidence": ai_result.confidence,
                })).unwrap_or_default()
            }
            OutputFormat::SRT => {
                // For SRT, we create a simple format
                format!("1\n00:00:00,000 --> 00:00:10,000\n{}\n", ai_result.translated_text)
            }
            OutputFormat::VTT => {
                format!("WEBVTT\n\n00:00:00.000 --> 00:00:10.000\n{}\n", ai_result.translated_text)
            }
            OutputFormat::TXT => {
                ai_result.translated_text.clone()
            }
        };

        // Upload output to drive
        let filename = format!("translation_{}.{}", task.task_no, request.output_format.as_str());
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
        ).await.map_err(|e| TranslationServiceError::DriveService(e.to_string()))?;

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
            artifact_type: Some("translation".to_string()),
            title: Some(format!("Translation: {} -> {}", 
                request.source_language.as_deref().unwrap_or("auto"),
                request.target_language
            )),
            voice_id: None,
            provider_code: Some(self.ai_engine.engine_type().as_str().to_string()),
            provider_asset_id: None,
            artifact_index: 0,
            format: Some(request.output_format.as_str().to_string()),
            mime_type: Some(mime_type.to_string()),
            duration_seconds: None,
            transcript_text: Some(ai_result.source_text.clone()),
            translation_text: Some(ai_result.translated_text.clone()),
            media_resource_json: media_resource.to_string(),
        }).await?;

        // Update task to completed
        let result = serde_json::json!({
            "sourceText": ai_result.source_text,
            "translatedText": ai_result.translated_text,
            "sourceLanguage": ai_result.source_language,
            "targetLanguage": ai_result.target_language,
            "confidence": ai_result.confidence,
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

        info!("Translation task completed: {}", task.task_no);
        Ok(())
    }
}

#[async_trait]
impl TranslationService for TranslationServiceImpl {
    async fn create_translation(
        &self,
        request: TranslationRequest,
    ) -> TranslationServiceResult<TranslationResponse> {
        info!("Creating translation task for user: {}", request.user_id);

        // Check idempotency
        if let Some(ref idempotency_key) = request.idempotency_key {
            let existing = self.task_repo.get_by_idempotency_key(
                request.tenant_id,
                OperationType::Translation.as_str(),
                idempotency_key,
            ).await?;

            if let Some(task) = existing {
                warn!("Task already exists with idempotency key: {}", idempotency_key);
                return Ok(TranslationResponse {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    estimated_duration_ms: None,
                });
            }
        }

        // Validate request
        if request.audio_url.is_empty() {
            return Err(TranslationServiceError::InvalidRequest {
                message: "Audio URL is required".to_string(),
            });
        }

        if request.target_language.is_empty() {
            return Err(TranslationServiceError::InvalidRequest {
                message: "Target language is required".to_string(),
            });
        }

        // Calculate input hash
        let input_hash = Self::calculate_input_hash(&request);

        // Create task
        let request_json = serde_json::to_string(&request)
            .map_err(|e| TranslationServiceError::Internal(e.to_string()))?;

        let task = self.task_repo.create(CreateTaskRequest {
            tenant_id: request.tenant_id,
            organization_id: request.organization_id,
            user_id: request.user_id,
            operation_type: OperationType::Translation,
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
            if let Err(e) = self_clone.process_translation_task(&task_clone, &request_clone).await {
                error!("Failed to process translation task: {}", e);
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

        Ok(TranslationResponse {
            task_id: task.id.to_string(),
            task_no: task.task_no,
            status: task.status,
            estimated_duration_ms: Some(8000), // Estimated 8 seconds
        })
    }

    async fn get_translation_result(
        &self,
        task_no: &str,
    ) -> TranslationServiceResult<Option<TranslationResult>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                if task.operation_type != OperationType::Translation.as_str() {
                    return Err(TranslationServiceError::InvalidRequest {
                        message: "Task is not a translation task".to_string(),
                    });
                }

                let result = if let Some(ref result_json) = task.result_json {
                    serde_json::from_str(result_json).ok()
                } else {
                    None
                };

                Ok(Some(TranslationResult {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    source_text: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("sourceText").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    translated_text: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("translatedText").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    source_language: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("sourceLanguage").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    target_language: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("targetLanguage").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                    confidence: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("confidence").and_then(|v| v.as_f64())
                    }),
                    segments: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("segments").and_then(|v| serde_json::from_value(v.clone()).ok())
                    }),
                    output_url: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("outputUrl").and_then(|v| v.as_str()).map(|s| s.to_string())
                    }),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_translation_detail(
        &self,
        task_no: &str,
    ) -> TranslationServiceResult<Option<TranslationTaskDetail>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                if task.operation_type != OperationType::Translation.as_str() {
                    return Err(TranslationServiceError::InvalidRequest {
                        message: "Task is not a translation task".to_string(),
                    });
                }

                let request_params: TranslationRequest = serde_json::from_str(&task.request_json)
                    .unwrap_or_else(|_| TranslationRequest {
                        tenant_id: task.tenant_id,
                        organization_id: task.organization_id,
                        user_id: task.user_id,
                        audio_url: String::new(),
                        source_language: None,
                        target_language: String::new(),
                        output_format: OutputFormat::JSON,
                        include_timestamps: true,
                        idempotency_key: None,
                    });

                let result = self.get_translation_result(task_no).await?;

                Ok(Some(TranslationTaskDetail {
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

    async fn list_translations(
        &self,
        request: TranslationListRequest,
    ) -> TranslationServiceResult<TranslationListResponse> {
        let filter = TaskFilter {
            tenant_id: Some(request.tenant_id),
            user_id: request.user_id,
            organization_id: None,
            operation_type: Some(OperationType::Translation),
            status: request.status.and_then(|s| TaskStatus::from_str(&s)),
            provider_code: None,
            created_after: None,
            created_before: None,
        };

        let result = self.task_repo.list(filter, request.limit, request.offset).await?;

        let mut tasks = Vec::new();
        for task in result.tasks {
            let detail = self.get_translation_detail(&task.task_no).await?;
            if let Some(detail) = detail {
                tasks.push(detail);
            }
        }

        Ok(TranslationListResponse {
            tasks,
            total: result.total,
            has_more: result.has_more,
            next_cursor: result.next_cursor,
        })
    }

    async fn cancel_translation(
        &self,
        task_no: &str,
    ) -> TranslationServiceResult<()> {
        let task = self.task_repo.get_by_task_no(task_no).await?
            .ok_or_else(|| TranslationServiceError::TaskNotFound {
                task_no: task_no.to_string(),
            })?;

        if task.operation_type != OperationType::Translation.as_str() {
            return Err(TranslationServiceError::InvalidRequest {
                message: "Task is not a translation task".to_string(),
            });
        }

        if task.status == TaskStatus::Succeeded.as_str() || task.status == TaskStatus::Failed.as_str() {
            return Err(TranslationServiceError::InvalidRequest {
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

    async fn get_supported_languages(
        &self,
    ) -> TranslationServiceResult<SupportedLanguagesResponse> {
        // Return a list of commonly supported languages
        let languages = vec![
            SupportedLanguage {
                code: "en".to_string(),
                name: "English".to_string(),
                native_name: "English".to_string(),
            },
            SupportedLanguage {
                code: "zh".to_string(),
                name: "Chinese".to_string(),
                native_name: "中文".to_string(),
            },
            SupportedLanguage {
                code: "ja".to_string(),
                name: "Japanese".to_string(),
                native_name: "日本語".to_string(),
            },
            SupportedLanguage {
                code: "ko".to_string(),
                name: "Korean".to_string(),
                native_name: "한국어".to_string(),
            },
            SupportedLanguage {
                code: "es".to_string(),
                name: "Spanish".to_string(),
                native_name: "Español".to_string(),
            },
            SupportedLanguage {
                code: "fr".to_string(),
                name: "French".to_string(),
                native_name: "Français".to_string(),
            },
            SupportedLanguage {
                code: "de".to_string(),
                name: "German".to_string(),
                native_name: "Deutsch".to_string(),
            },
            SupportedLanguage {
                code: "it".to_string(),
                name: "Italian".to_string(),
                native_name: "Italiano".to_string(),
            },
            SupportedLanguage {
                code: "pt".to_string(),
                name: "Portuguese".to_string(),
                native_name: "Português".to_string(),
            },
            SupportedLanguage {
                code: "ru".to_string(),
                name: "Russian".to_string(),
                native_name: "Русский".to_string(),
            },
            SupportedLanguage {
                code: "ar".to_string(),
                name: "Arabic".to_string(),
                native_name: "العربية".to_string(),
            },
            SupportedLanguage {
                code: "hi".to_string(),
                name: "Hindi".to_string(),
                native_name: "हिन्दी".to_string(),
            },
        ];

        Ok(SupportedLanguagesResponse { languages })
    }
}
