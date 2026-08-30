//! Unit tests for speech service

use async_trait::async_trait;
use std::sync::Arc;
use chrono::Utc;
use uuid::Uuid;

use sdkwork_audio_speech_service::error::*;
use sdkwork_audio_speech_service::models::*;
use sdkwork_audio_speech_service::service::*;
use sdkwork_audio_generation_service_port::entities::task::*;
use sdkwork_audio_generation_service_port::entities::artifact::*;
use sdkwork_audio_generation_service_port::entities::voice::*;
use sdkwork_audio_generation_service_port::repositories::task::*;
use sdkwork_audio_generation_service_port::repositories::artifact::*;
use sdkwork_audio_generation_service_port::repositories::voice::*;
use sdkwork_audio_ai_engine_rust::*;
// Aliases for the ai-engine request/result types (distinct from the service's own models)
use sdkwork_audio_ai_engine_rust::{
    SpeechSynthesisRequest as EngineSpeechSynthesisRequest,
    SpeechSynthesisResult as EngineSpeechSynthesisResult,
};
use sdkwork_audio_artifact_drive_service::*;

// Mock Task Repository
struct MockTaskRepository {
    tasks: std::sync::Mutex<Vec<AudioGenerationTask>>,
}

impl MockTaskRepository {
    fn new() -> Self {
        Self {
            tasks: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl TaskRepository for MockTaskRepository {
    async fn create(&self, request: CreateTaskRequest) -> Result<AudioGenerationTask, sqlx::Error> {
        let task = AudioGenerationTask {
            id: 1,
            task_no: Uuid::new_v4().to_string(),
            tenant_id: request.tenant_id,
            organization_id: request.organization_id,
            user_id: request.user_id,
            operation_type: request.operation_type.as_str().to_string(),
            provider_code: request.provider_code,
            provider_route_id: request.provider_route_id,
            model: request.model,
            provider_task_id: None,
            idempotency_key: request.idempotency_key,
            input_hash: None,
            status: TaskStatus::Queued.as_str().to_string(),
            progress: 0,
            request_json: request.request_json,
            normalized_options_json: None,
            provider_request_json: None,
            provider_response_json: None,
            result_json: None,
            error_code: None,
            error_message: None,
            callback_url: request.callback_url,
            callback_status: None,
            submitted_at: None,
            completed_at: None,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            version: 0,
        };
        self.tasks.lock().unwrap().push(task.clone());
        Ok(task)
    }

    async fn get_by_id(&self, id: i64) -> Result<Option<AudioGenerationTask>, sqlx::Error> {
        Ok(self.tasks.lock().unwrap().iter().find(|t| t.id == id).cloned())
    }

    async fn get_by_task_no(&self, task_no: &str) -> Result<Option<AudioGenerationTask>, sqlx::Error> {
        Ok(self.tasks.lock().unwrap().iter().find(|t| t.task_no == task_no).cloned())
    }

    async fn get_by_idempotency_key(
        &self,
        tenant_id: i64,
        operation_type: &str,
        idempotency_key: &str,
    ) -> Result<Option<AudioGenerationTask>, sqlx::Error> {
        Ok(self.tasks.lock().unwrap().iter().find(|t| {
            t.tenant_id == tenant_id
                && t.operation_type == operation_type
                && t.idempotency_key.as_deref() == Some(idempotency_key)
        }).cloned())
    }

    async fn update(&self, id: i64, request: UpdateTaskRequest) -> Result<AudioGenerationTask, sqlx::Error> {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            if let Some(status) = request.status {
                task.status = status.as_str().to_string();
            }
            if let Some(progress) = request.progress {
                task.progress = progress;
            }
            if let Some(result_json) = request.result_json {
                task.result_json = Some(result_json);
            }
            if let Some(completed_at) = request.completed_at {
                task.completed_at = Some(completed_at);
            }
            if let Some(error_code) = request.error_code {
                task.error_code = Some(error_code);
            }
            if let Some(error_message) = request.error_message {
                task.error_message = Some(error_message);
            }
            task.updated_at = Utc::now();
            task.version += 1;
            Ok(task.clone())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    async fn list(&self, filter: TaskFilter, limit: i64, offset: i64) -> Result<TaskListResult, sqlx::Error> {
        let tasks = self.tasks.lock().unwrap();
        let filtered: Vec<AudioGenerationTask> = tasks.iter()
            .filter(|t| {
                if let Some(tenant_id) = filter.tenant_id {
                    if t.tenant_id != tenant_id {
                        return false;
                    }
                }
                if let Some(ref operation_type) = filter.operation_type {
                    if t.operation_type != operation_type.as_str() {
                        return false;
                    }
                }
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();

        let total = filtered.len() as i64;
        let has_more = (offset + limit) < total;

        Ok(TaskListResult {
            tasks: filtered,
            total,
            has_more,
            next_cursor: if has_more { Some((offset + limit).to_string()) } else { None },
        })
    }

    async fn count(&self, filter: TaskFilter) -> Result<i64, sqlx::Error> {
        let tasks = self.tasks.lock().unwrap();
        let count = tasks.iter()
            .filter(|t| {
                if let Some(tenant_id) = filter.tenant_id {
                    if t.tenant_id != tenant_id {
                        return false;
                    }
                }
                if let Some(ref operation_type) = filter.operation_type {
                    if t.operation_type != operation_type.as_str() {
                        return false;
                    }
                }
                true
            })
            .count();
        Ok(count as i64)
    }
}

// Mock Artifact Repository
struct MockArtifactRepository {
    artifacts: std::sync::Mutex<Vec<AudioArtifact>>,
}

impl MockArtifactRepository {
    fn new() -> Self {
        Self {
            artifacts: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ArtifactRepository for MockArtifactRepository {
    async fn create(&self, request: CreateArtifactRequest) -> Result<AudioArtifact, sqlx::Error> {
        let artifact = AudioArtifact {
            id: 1,
            artifact_no: Uuid::new_v4().to_string(),
            task_id: request.task_id,
            request_id: request.request_id,
            kind: request.kind.as_str().to_string(),
            artifact_type: request.artifact_type,
            title: request.title,
            voice_id: request.voice_id,
            provider_code: request.provider_code,
            provider_asset_id: request.provider_asset_id,
            artifact_index: request.artifact_index,
            format: request.format,
            mime_type: request.mime_type,
            duration_seconds: request.duration_seconds,
            checksum_json: None,
            transcript_text: request.transcript_text,
            translation_text: request.translation_text,
            media_resource_json: request.media_resource_json,
            resource_snapshot_json: None,
            status: "pending".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            version: 0,
        };
        self.artifacts.lock().unwrap().push(artifact.clone());
        Ok(artifact)
    }

    async fn get_by_id(&self, id: i64) -> Result<Option<AudioArtifact>, sqlx::Error> {
        Ok(self.artifacts.lock().unwrap().iter().find(|a| a.id == id).cloned())
    }

    async fn get_by_artifact_no(&self, artifact_no: &str) -> Result<Option<AudioArtifact>, sqlx::Error> {
        Ok(self.artifacts.lock().unwrap().iter().find(|a| a.artifact_no == artifact_no).cloned())
    }

    async fn list_by_task(&self, task_id: i64) -> Result<Vec<AudioArtifact>, sqlx::Error> {
        Ok(self.artifacts.lock().unwrap().iter().filter(|a| a.task_id == Some(task_id)).cloned().collect())
    }

    async fn list(&self, filter: ArtifactFilter, limit: i64, offset: i64) -> Result<ArtifactListResult, sqlx::Error> {
        let artifacts = self.artifacts.lock().unwrap();
        let filtered: Vec<AudioArtifact> = artifacts.iter()
            .filter(|a| {
                if let Some(task_id) = filter.task_id {
                    if a.task_id != Some(task_id) {
                        return false;
                    }
                }
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();

        let total = filtered.len() as i64;
        let has_more = (offset + limit) < total;

        Ok(ArtifactListResult {
            artifacts: filtered,
            total,
            has_more,
            next_cursor: if has_more { Some((offset + limit).to_string()) } else { None },
        })
    }

    async fn count(&self, filter: ArtifactFilter) -> Result<i64, sqlx::Error> {
        let artifacts = self.artifacts.lock().unwrap();
        let count = artifacts.iter()
            .filter(|a| {
                if let Some(task_id) = filter.task_id {
                    if a.task_id != Some(task_id) {
                        return false;
                    }
                }
                true
            })
            .count();
        Ok(count as i64)
    }
}

// Mock Voice Repository
struct MockVoiceRepository {
    voices: std::sync::Mutex<Vec<AudioVoice>>,
}

impl MockVoiceRepository {
    fn new() -> Self {
        Self {
            voices: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl VoiceRepository for MockVoiceRepository {
    async fn create(&self, request: CreateVoiceRequest) -> Result<AudioVoice, sqlx::Error> {
        let voice = AudioVoice {
            id: 1,
            voice_no: Uuid::new_v4().to_string(),
            tenant_id: request.tenant_id,
            organization_id: request.organization_id,
            user_id: request.user_id,
            name: request.name,
            description: request.description,
            language: request.language,
            gender: request.gender,
            voice_type: request.voice_type.as_str().to_string(),
            provider_code: request.provider_code,
            provider_voice_id: request.provider_voice_id,
            provider_config_json: request.provider_config_json,
            clone_reference_url: request.clone_reference_url,
            clone_parameters_json: request.clone_parameters_json,
            status: VoiceStatus::Active.as_str().to_string(),
            is_public: request.is_public,
            usage_count: 0,
            quality_score: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            version: 0,
        };
        self.voices.lock().unwrap().push(voice.clone());
        Ok(voice)
    }

    async fn get_by_id(&self, id: i64) -> Result<Option<AudioVoice>, sqlx::Error> {
        Ok(self.voices.lock().unwrap().iter().find(|v| v.id == id).cloned())
    }

    async fn get_by_voice_no(&self, voice_no: &str) -> Result<Option<AudioVoice>, sqlx::Error> {
        Ok(self.voices.lock().unwrap().iter().find(|v| v.voice_no == voice_no).cloned())
    }

    async fn update(&self, id: i64, request: UpdateVoiceRequest) -> Result<AudioVoice, sqlx::Error> {
        let mut voices = self.voices.lock().unwrap();
        if let Some(voice) = voices.iter_mut().find(|v| v.id == id) {
            if let Some(name) = request.name {
                voice.name = name;
            }
            if let Some(description) = request.description {
                voice.description = Some(description);
            }
            voice.updated_at = Utc::now();
            voice.version += 1;
            Ok(voice.clone())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    async fn list(&self, filter: VoiceFilter, limit: i64, offset: i64) -> Result<VoiceListResult, sqlx::Error> {
        let voices = self.voices.lock().unwrap();
        let filtered: Vec<AudioVoice> = voices.iter()
            .filter(|v| {
                if let Some(tenant_id) = filter.tenant_id {
                    if v.tenant_id != tenant_id {
                        return false;
                    }
                }
                if let Some(ref language) = filter.language {
                    if v.language != *language {
                        return false;
                    }
                }
                true
            })
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();

        let total = filtered.len() as i64;
        let has_more = (offset + limit) < total;

        Ok(VoiceListResult {
            voices: filtered,
            total,
            has_more,
            next_cursor: if has_more { Some((offset + limit).to_string()) } else { None },
        })
    }

    async fn count(&self, filter: VoiceFilter) -> Result<i64, sqlx::Error> {
        let voices = self.voices.lock().unwrap();
        let count = voices.iter()
            .filter(|v| {
                if let Some(tenant_id) = filter.tenant_id {
                    if v.tenant_id != tenant_id {
                        return false;
                    }
                }
                true
            })
            .count();
        Ok(count as i64)
    }
}

// Mock AI Engine
struct MockAiEngine;

#[async_trait]
impl AudioAiEngine for MockAiEngine {
    fn engine_type(&self) -> AiEngineType {
        AiEngineType::Custom
    }

    fn engine_name(&self) -> &str {
        "Mock AI Engine"
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn synthesize_speech(
        &self,
        _request: EngineSpeechSynthesisRequest,
    ) -> Result<EngineSpeechSynthesisResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(EngineSpeechSynthesisResult {
            audio_data: vec![0; 1024],
            mime_type: "audio/mpeg".to_string(),
            duration_ms: 2500,
            sample_rate: 24000,
            channels: 1,
        })
    }

    async fn transcribe_audio(
        &self,
        _request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!()
    }

    async fn translate_audio(
        &self,
        _request: TranslationRequest,
    ) -> Result<TranslationResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!()
    }

    async fn generate_sound_effect(
        &self,
        _request: SoundEffectRequest,
    ) -> Result<SoundEffectResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!()
    }
}

// Mock Drive Service
struct MockDriveService;

#[async_trait]
impl AudioArtifactDriveService for MockDriveService {
    async fn upload_artifact(
        &self,
        _tenant_id: i64,
        _user_id: i64,
        _artifact_data: &[u8],
        _mime_type: &str,
        _filename: &str,
    ) -> Result<DriveUploadResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(DriveUploadResult {
            drive_space_id: "space-123".to_string(),
            drive_node_id: "node-456".to_string(),
            drive_resource_uri: "drive://spaces/space-123/nodes/node-456".to_string(),
            upload_item_id: Some("item-789".to_string()),
            upload_session_id: Some("session-101".to_string()),
        })
    }

    async fn get_download_url(
        &self,
        _drive_space_id: &str,
        _drive_node_id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("https://example.com/download".to_string())
    }

    async fn delete_artifact(
        &self,
        _drive_space_id: &str,
        _drive_node_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

// Helper function to create service
fn create_service() -> SpeechServiceImpl {
    let task_repo = Arc::new(MockTaskRepository::new());
    let artifact_repo = Arc::new(MockArtifactRepository::new());
    let voice_repo = Arc::new(MockVoiceRepository::new());
    let ai_engine = Arc::new(MockAiEngine);
    let drive_service = Arc::new(MockDriveService);

    SpeechServiceImpl::new(task_repo, artifact_repo, voice_repo, ai_engine, drive_service)
}

fn create_request() -> sdkwork_audio_speech_service::models::SpeechSynthesisRequest {
    sdkwork_audio_speech_service::models::SpeechSynthesisRequest {
        tenant_id: 100_001,
        organization_id: 0,
        user_id: 1,
        text: "Hello, world!".to_string(),
        text_format: TextFormat::Plain,
        language: Some("en".to_string()),
        voice_id: Some("voice-123".to_string()),
        speed: 1.0,
        pitch: 1.0,
        volume: 1.0,
        emotion: Some("neutral".to_string()),
        emotion_intensity: 0.5,
        audio_format: AudioFormat::MP3,
        sample_rate: 24000,
        idempotency_key: None,
    }
}

#[tokio::test]
async fn test_create_speech_success() {
    let service = create_service();
    let request = create_request();

    let result = service.create_speech(request).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert!(!response.task_id.is_empty());
    assert!(!response.task_no.is_empty());
    assert_eq!(response.status, "queued");
    assert_eq!(response.estimated_duration_ms, Some(2000));
}

#[tokio::test]
async fn test_create_speech_with_idempotency_key() {
    let service = create_service();
    let mut request = create_request();
    request.idempotency_key = Some("key-123".to_string());

    let result1 = service.create_speech(request.clone()).await;
    assert!(result1.is_ok());

    let result2 = service.create_speech(request).await;
    assert!(result2.is_ok());

    let response1 = result1.unwrap();
    let response2 = result2.unwrap();
    assert_eq!(response1.task_no, response2.task_no);
}

#[tokio::test]
async fn test_list_voices_empty() {
    let service = create_service();

    let request = VoiceListRequest {
        tenant_id: 100_001,
        user_id: None,
        language: None,
        gender: None,
        voice_type: None,
        limit: 20,
        offset: 0,
    };

    let result = service.list_voices(request).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert_eq!(response.voices.len(), 0);
    assert_eq!(response.total, 0);
    assert_eq!(response.has_more, false);
}

#[tokio::test]
async fn test_cancel_speech_not_found() {
    let service = create_service();

    let result = service.cancel_speech("non-existent").await;
    assert!(result.is_err());

    match result.unwrap_err() {
        SpeechServiceError::TaskNotFound { task_no } => {
            assert_eq!(task_no, "non-existent");
        }
        _ => panic!("Expected TaskNotFound error"),
    }
}

#[test]
fn test_calculate_input_hash() {
    let request1 = create_request();
    let request2 = create_request();

    // Same request should produce same hash
    let hash1 = sdkwork_audio_speech_service::service::SpeechServiceImpl::calculate_input_hash(&request1);
    let hash2 = sdkwork_audio_speech_service::service::SpeechServiceImpl::calculate_input_hash(&request2);
    assert_eq!(hash1, hash2);

    // Different text should produce different hash
    let mut request3 = create_request();
    request3.text = "Different text".to_string();
    let hash3 = sdkwork_audio_speech_service::service::SpeechServiceImpl::calculate_input_hash(&request3);
    assert_ne!(hash1, hash3);
}
