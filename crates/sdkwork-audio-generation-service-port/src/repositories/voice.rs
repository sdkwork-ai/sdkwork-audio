//! Voice repository port

use async_trait::async_trait;

use crate::entities::voice::*;

/// Voice repository port trait
#[async_trait]
pub trait VoiceRepository {
    async fn create(&self, request: CreateVoiceRequest) -> Result<AudioVoice, sqlx::Error>;
    async fn get_by_id(&self, id: i64) -> Result<Option<AudioVoice>, sqlx::Error>;
    async fn get_by_voice_no(&self, voice_no: &str) -> Result<Option<AudioVoice>, sqlx::Error>;
    async fn update(&self, id: i64, request: UpdateVoiceRequest) -> Result<AudioVoice, sqlx::Error>;
    async fn list(&self, filter: VoiceFilter, limit: i64, offset: i64) -> Result<VoiceListResult, sqlx::Error>;
    async fn count(&self, filter: VoiceFilter) -> Result<i64, sqlx::Error>;
}