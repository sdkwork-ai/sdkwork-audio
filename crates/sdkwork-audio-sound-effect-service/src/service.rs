//! Sound effect service implementation

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::error::{SoundEffectServiceError, SoundEffectServiceResult};
use crate::models::*;
use sdkwork_audio_artifact_drive_service::AudioArtifactDriveService;
use sdkwork_audio_generation_service_port::entities::artifact::*;
use sdkwork_audio_generation_service_port::entities::task::*;
use sdkwork_audio_generation_service_port::repositories::artifact::ArtifactRepository;
use sdkwork_audio_generation_service_port::repositories::task::TaskRepository;
use sdkwork_audio_sound_effect_generation_service::SoundEffectGenerationServicePort;
use sdkwork_audio_sound_effect_provider_spi::{SoundEffectGenerationCommand, SoundEffectVendorId};

/// Sound effect service trait
#[async_trait]
pub trait SoundEffectService {
    /// Create a sound effect task
    async fn create_sound_effect(
        &self,
        request: SoundEffectRequest,
    ) -> SoundEffectServiceResult<SoundEffectResponse>;

    /// Get sound effect result
    async fn get_sound_effect_result(
        &self,
        task_no: &str,
    ) -> SoundEffectServiceResult<Option<SoundEffectResult>>;

    /// Get sound effect task detail
    async fn get_sound_effect_detail(
        &self,
        task_no: &str,
    ) -> SoundEffectServiceResult<Option<SoundEffectTaskDetail>>;

    /// List sound effect tasks
    async fn list_sound_effects(
        &self,
        request: SoundEffectListRequest,
    ) -> SoundEffectServiceResult<SoundEffectListResponse>;

    /// Cancel a sound effect task
    async fn cancel_sound_effect(&self, task_no: &str) -> SoundEffectServiceResult<()>;

    /// Get sound effect presets
    async fn get_presets(&self) -> SoundEffectServiceResult<SoundEffectPresetsResponse>;

    /// Get sound effect categories
    async fn get_categories(&self) -> SoundEffectServiceResult<SoundEffectCategoriesResponse>;
}

/// Sound effect service implementation
pub struct SoundEffectServiceImpl {
    task_repo: Arc<dyn TaskRepository + Send + Sync>,
    artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
    generation_service: Arc<dyn SoundEffectGenerationServicePort>,
    drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
}

impl SoundEffectServiceImpl {
    pub fn new(
        task_repo: Arc<dyn TaskRepository + Send + Sync>,
        artifact_repo: Arc<dyn ArtifactRepository + Send + Sync>,
        generation_service: Arc<dyn SoundEffectGenerationServicePort>,
        drive_service: Arc<dyn AudioArtifactDriveService + Send + Sync>,
    ) -> Self {
        Self {
            task_repo,
            artifact_repo,
            generation_service,
            drive_service,
        }
    }

    /// Calculate input hash for idempotency
    fn calculate_input_hash(request: &SoundEffectRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(request.description.as_bytes());
        if let Some(duration) = request.duration_ms {
            hasher.update(duration.to_string().as_bytes());
        }
        if let Some(ref style) = request.style {
            hasher.update(style.as_bytes());
        }
        hasher.update(request.intensity.to_string().as_bytes());
        hasher.update(request.audio_format.as_str().as_bytes());
        hasher.update(request.sample_rate.to_string().as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Process sound effect task
    async fn process_sound_effect_task(
        &self,
        task: &AudioGenerationTask,
        request: &SoundEffectRequest,
    ) -> SoundEffectServiceResult<()> {
        info!("Processing sound effect task: {}", task.task_no);

        // Update task status to running
        self.task_repo
            .update(
                task.id,
                UpdateTaskRequest {
                    status: Some(TaskStatus::Running),
                    progress: Some(10),
                    ..Default::default()
                },
            )
            .await?;

        let vendor = request.vendor.as_deref().unwrap_or("custom");
        let generation_command = SoundEffectGenerationCommand {
            vendor: SoundEffectVendorId::new(vendor)
                .map_err(|error| SoundEffectServiceError::AiEngine(error.to_string()))?,
            model: request.model.clone(),
            description: request.description.clone(),
            duration_ms: request.duration_ms,
            style: request.style.clone(),
            intensity: request.intensity,
            format: request.audio_format.as_str().to_string(),
            sample_rate: request.sample_rate,
            idempotency_key: request.idempotency_key.clone(),
            vendor_parameters: request.vendor_parameters.clone(),
        };

        // Update progress
        self.task_repo
            .update(
                task.id,
                UpdateTaskRequest {
                    progress: Some(30),
                    ..Default::default()
                },
            )
            .await?;

        let submission = self
            .generation_service
            .generate(generation_command)
            .await
            .map_err(|error| SoundEffectServiceError::AiEngine(error.to_string()))?;
        let provider_vendor = submission.vendor.clone();
        let generated = submission.output;

        // Update progress
        self.task_repo
            .update(
                task.id,
                UpdateTaskRequest {
                    progress: Some(60),
                    ..Default::default()
                },
            )
            .await?;

        // Upload audio to drive
        let filename = format!(
            "sound_effect_{}.{}",
            task.task_no,
            request.audio_format.as_str()
        );
        let mime_type = request.audio_format.mime_type();

        let drive_result = self
            .drive_service
            .upload_artifact(
                task.tenant_id,
                task.user_id,
                &generated.audio_data,
                mime_type,
                &filename,
            )
            .await
            .map_err(|e| SoundEffectServiceError::DriveService(e.to_string()))?;

        // Update progress
        self.task_repo
            .update(
                task.id,
                UpdateTaskRequest {
                    progress: Some(80),
                    ..Default::default()
                },
            )
            .await?;

        // Create artifact
        let media_resource = serde_json::json!({
            "source": "drive",
            "driveSpaceId": drive_result.drive_space_id,
            "driveNodeId": drive_result.drive_node_id,
            "driveResourceUri": drive_result.drive_resource_uri,
        });

        self.artifact_repo
            .create(CreateArtifactRequest {
                task_id: Some(task.id),
                request_id: None,
                kind: ArtifactKind::Audio,
                artifact_type: Some("sound_effect".to_string()),
                title: Some(format!(
                    "Sound effect: {}",
                    &request.description[..std::cmp::min(50, request.description.len())]
                )),
                voice_id: None,
                provider_code: Some(provider_vendor),
                provider_asset_id: None,
                artifact_index: 0,
                format: Some(request.audio_format.as_str().to_string()),
                mime_type: Some(mime_type.to_string()),
                duration_seconds: Some((generated.duration_ms / 1000) as i32),
                transcript_text: None,
                translation_text: None,
                media_resource_json: media_resource.to_string(),
            })
            .await?;

        // Update task to completed
        let result = serde_json::json!({
            "durationMs": generated.duration_ms,
            "sampleRate": generated.sample_rate,
            "channels": generated.channels,
            "fileSizeBytes": generated.audio_data.len(),
            "format": request.audio_format.as_str(),
            "mimeType": mime_type,
            "driveSpaceId": drive_result.drive_space_id,
            "driveNodeId": drive_result.drive_node_id,
            "audioUrl": drive_result.drive_resource_uri,
        });

        self.task_repo
            .update(
                task.id,
                UpdateTaskRequest {
                    status: Some(TaskStatus::Succeeded),
                    progress: Some(100),
                    result_json: Some(result.to_string()),
                    completed_at: Some(chrono::Utc::now()),
                    ..Default::default()
                },
            )
            .await?;

        info!("Sound effect task completed: {}", task.task_no);
        Ok(())
    }
}

#[async_trait]
impl SoundEffectService for SoundEffectServiceImpl {
    async fn create_sound_effect(
        &self,
        request: SoundEffectRequest,
    ) -> SoundEffectServiceResult<SoundEffectResponse> {
        info!("Creating sound effect task for user: {}", request.user_id);

        // Check idempotency
        if let Some(ref idempotency_key) = request.idempotency_key {
            let existing = self
                .task_repo
                .get_by_idempotency_key(
                    request.tenant_id,
                    OperationType::SoundEffect.as_str(),
                    idempotency_key,
                )
                .await?;

            if let Some(task) = existing {
                warn!(
                    "Task already exists with idempotency key: {}",
                    idempotency_key
                );
                return Ok(SoundEffectResponse {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    estimated_duration_ms: None,
                });
            }
        }

        // Validate request
        if request.description.is_empty() {
            return Err(SoundEffectServiceError::InvalidRequest {
                message: "Description is required".to_string(),
            });
        }

        if request.intensity < 0.0 || request.intensity > 1.0 {
            return Err(SoundEffectServiceError::InvalidRequest {
                message: "Intensity must be between 0.0 and 1.0".to_string(),
            });
        }

        // Calculate input hash
        let input_hash = Self::calculate_input_hash(&request);

        // Create task
        let request_json = serde_json::to_string(&request)
            .map_err(|e| SoundEffectServiceError::Internal(e.to_string()))?;

        let task = self
            .task_repo
            .create(CreateTaskRequest {
                tenant_id: request.tenant_id,
                organization_id: request.organization_id,
                user_id: request.user_id,
                operation_type: OperationType::SoundEffect,
                provider_code: request
                    .vendor
                    .clone()
                    .unwrap_or_else(|| "custom".to_string()),
                provider_route_id: None,
                model: None,
                idempotency_key: request.idempotency_key.clone(),
                request_json: request_json.clone(),
                callback_url: None,
            })
            .await?;

        // Process task asynchronously
        let task_clone = task.clone();
        let request_clone = request.clone();
        let self_clone = Self {
            task_repo: self.task_repo.clone(),
            artifact_repo: self.artifact_repo.clone(),
            generation_service: self.generation_service.clone(),
            drive_service: self.drive_service.clone(),
        };

        tokio::spawn(async move {
            if let Err(e) = self_clone
                .process_sound_effect_task(&task_clone, &request_clone)
                .await
            {
                error!("Failed to process sound effect task: {}", e);
                // Update task to failed
                let _ = self_clone
                    .task_repo
                    .update(
                        task_clone.id,
                        UpdateTaskRequest {
                            status: Some(TaskStatus::Failed),
                            error_code: Some("PROCESSING_ERROR".to_string()),
                            error_message: Some(e.to_string()),
                            completed_at: Some(chrono::Utc::now()),
                            ..Default::default()
                        },
                    )
                    .await;
            }
        });

        Ok(SoundEffectResponse {
            task_id: task.id.to_string(),
            task_no: task.task_no,
            status: task.status,
            estimated_duration_ms: Some(3000), // Estimated 3 seconds
        })
    }

    async fn get_sound_effect_result(
        &self,
        task_no: &str,
    ) -> SoundEffectServiceResult<Option<SoundEffectResult>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                if task.operation_type != OperationType::SoundEffect.as_str() {
                    return Err(SoundEffectServiceError::InvalidRequest {
                        message: "Task is not a sound effect task".to_string(),
                    });
                }

                let result = if let Some(ref result_json) = task.result_json {
                    serde_json::from_str(result_json).ok()
                } else {
                    None
                };

                Ok(Some(SoundEffectResult {
                    task_id: task.id.to_string(),
                    task_no: task.task_no,
                    status: task.status,
                    audio_url: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("audioUrl")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }),
                    duration_ms: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("durationMs").and_then(|v| v.as_u64())
                    }),
                    file_size_bytes: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("fileSizeBytes").and_then(|v| v.as_u64())
                    }),
                    mime_type: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("mimeType")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }),
                    format: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("format")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }),
                    sample_rate: result.as_ref().and_then(|r: &serde_json::Value| {
                        r.get("sampleRate")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u32)
                    }),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_sound_effect_detail(
        &self,
        task_no: &str,
    ) -> SoundEffectServiceResult<Option<SoundEffectTaskDetail>> {
        let task = self.task_repo.get_by_task_no(task_no).await?;

        match task {
            Some(task) => {
                if task.operation_type != OperationType::SoundEffect.as_str() {
                    return Err(SoundEffectServiceError::InvalidRequest {
                        message: "Task is not a sound effect task".to_string(),
                    });
                }

                let request_params: SoundEffectRequest = serde_json::from_str(&task.request_json)
                    .unwrap_or_else(|_| SoundEffectRequest {
                        tenant_id: task.tenant_id,
                        organization_id: task.organization_id,
                        user_id: task.user_id,
                        vendor: None,
                        model: None,
                        description: String::new(),
                        duration_ms: None,
                        style: None,
                        intensity: 0.5,
                        audio_format: AudioFormat::WAV,
                        sample_rate: 44100,
                        idempotency_key: None,
                        vendor_parameters: None,
                    });

                let result = self.get_sound_effect_result(task_no).await?;

                Ok(Some(SoundEffectTaskDetail {
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

    async fn list_sound_effects(
        &self,
        request: SoundEffectListRequest,
    ) -> SoundEffectServiceResult<SoundEffectListResponse> {
        let filter = TaskFilter {
            tenant_id: Some(request.tenant_id),
            user_id: request.user_id,
            organization_id: None,
            operation_type: Some(OperationType::SoundEffect),
            status: request.status.and_then(|s| TaskStatus::from_str(&s)),
            provider_code: None,
            created_after: None,
            created_before: None,
        };

        let result = self
            .task_repo
            .list(filter, request.limit, request.offset)
            .await?;

        let mut tasks = Vec::new();
        for task in result.tasks {
            let detail = self.get_sound_effect_detail(&task.task_no).await?;
            if let Some(detail) = detail {
                tasks.push(detail);
            }
        }

        Ok(SoundEffectListResponse {
            tasks,
            total: result.total,
            has_more: result.has_more,
            next_cursor: result.next_cursor,
        })
    }

    async fn cancel_sound_effect(&self, task_no: &str) -> SoundEffectServiceResult<()> {
        let task = self
            .task_repo
            .get_by_task_no(task_no)
            .await?
            .ok_or_else(|| SoundEffectServiceError::TaskNotFound {
                task_no: task_no.to_string(),
            })?;

        if task.operation_type != OperationType::SoundEffect.as_str() {
            return Err(SoundEffectServiceError::InvalidRequest {
                message: "Task is not a sound effect task".to_string(),
            });
        }

        if task.status == TaskStatus::Succeeded.as_str()
            || task.status == TaskStatus::Failed.as_str()
        {
            return Err(SoundEffectServiceError::InvalidRequest {
                message: "Cannot cancel completed task".to_string(),
            });
        }

        self.task_repo
            .update(
                task.id,
                UpdateTaskRequest {
                    status: Some(TaskStatus::Cancelled),
                    completed_at: Some(chrono::Utc::now()),
                    ..Default::default()
                },
            )
            .await?;

        Ok(())
    }

    async fn get_presets(&self) -> SoundEffectServiceResult<SoundEffectPresetsResponse> {
        let presets = vec![
            SoundEffectPreset {
                preset_id: "rain".to_string(),
                name: "Rain".to_string(),
                description: "Gentle rain falling on a window".to_string(),
                category: "nature".to_string(),
                tags: vec![
                    "rain".to_string(),
                    "weather".to_string(),
                    "ambient".to_string(),
                ],
                default_duration_ms: 5000,
                default_style: Some("gentle".to_string()),
            },
            SoundEffectPreset {
                preset_id: "thunder".to_string(),
                name: "Thunder".to_string(),
                description: "Thunder rumbling in the distance".to_string(),
                category: "nature".to_string(),
                tags: vec![
                    "thunder".to_string(),
                    "storm".to_string(),
                    "weather".to_string(),
                ],
                default_duration_ms: 3000,
                default_style: Some("dramatic".to_string()),
            },
            SoundEffectPreset {
                preset_id: "ocean".to_string(),
                name: "Ocean Waves".to_string(),
                description: "Ocean waves crashing on the shore".to_string(),
                category: "nature".to_string(),
                tags: vec![
                    "ocean".to_string(),
                    "waves".to_string(),
                    "beach".to_string(),
                ],
                default_duration_ms: 8000,
                default_style: Some("calm".to_string()),
            },
            SoundEffectPreset {
                preset_id: "footsteps".to_string(),
                name: "Footsteps".to_string(),
                description: "Footsteps walking on a hard surface".to_string(),
                category: "human".to_string(),
                tags: vec![
                    "footsteps".to_string(),
                    "walking".to_string(),
                    "human".to_string(),
                ],
                default_duration_ms: 2000,
                default_style: Some("normal".to_string()),
            },
            SoundEffectPreset {
                preset_id: "door".to_string(),
                name: "Door Opening".to_string(),
                description: "Door opening and closing".to_string(),
                category: "household".to_string(),
                tags: vec![
                    "door".to_string(),
                    "opening".to_string(),
                    "household".to_string(),
                ],
                default_duration_ms: 1500,
                default_style: Some("normal".to_string()),
            },
            SoundEffectPreset {
                preset_id: "click".to_string(),
                name: "Mouse Click".to_string(),
                description: "Mouse button click sound".to_string(),
                category: "ui".to_string(),
                tags: vec!["click".to_string(), "mouse".to_string(), "ui".to_string()],
                default_duration_ms: 100,
                default_style: Some("sharp".to_string()),
            },
            SoundEffectPreset {
                preset_id: "notification".to_string(),
                name: "Notification".to_string(),
                description: "Notification bell sound".to_string(),
                category: "ui".to_string(),
                tags: vec![
                    "notification".to_string(),
                    "bell".to_string(),
                    "alert".to_string(),
                ],
                default_duration_ms: 500,
                default_style: Some("pleasant".to_string()),
            },
            SoundEffectPreset {
                preset_id: "explosion".to_string(),
                name: "Explosion".to_string(),
                description: "Large explosion sound effect".to_string(),
                category: "action".to_string(),
                tags: vec![
                    "explosion".to_string(),
                    "boom".to_string(),
                    "action".to_string(),
                ],
                default_duration_ms: 2000,
                default_style: Some("dramatic".to_string()),
            },
        ];

        Ok(SoundEffectPresetsResponse { presets })
    }

    async fn get_categories(&self) -> SoundEffectServiceResult<SoundEffectCategoriesResponse> {
        let categories = vec![
            SoundEffectCategory {
                category_id: "nature".to_string(),
                name: "Nature".to_string(),
                description: "Natural environment sounds".to_string(),
                preset_count: 3,
            },
            SoundEffectCategory {
                category_id: "human".to_string(),
                name: "Human".to_string(),
                description: "Human-made sounds".to_string(),
                preset_count: 1,
            },
            SoundEffectCategory {
                category_id: "household".to_string(),
                name: "Household".to_string(),
                description: "Household item sounds".to_string(),
                preset_count: 1,
            },
            SoundEffectCategory {
                category_id: "ui".to_string(),
                name: "UI".to_string(),
                description: "User interface sounds".to_string(),
                preset_count: 2,
            },
            SoundEffectCategory {
                category_id: "action".to_string(),
                name: "Action".to_string(),
                description: "Action and adventure sounds".to_string(),
                preset_count: 1,
            },
        ];

        Ok(SoundEffectCategoriesResponse { categories })
    }
}
