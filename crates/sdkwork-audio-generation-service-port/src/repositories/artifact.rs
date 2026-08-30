//! Artifact repository port

use async_trait::async_trait;

use crate::entities::artifact::*;

/// Artifact repository port trait
#[async_trait]
pub trait ArtifactRepository {
    async fn create(&self, request: CreateArtifactRequest) -> Result<AudioArtifact, sqlx::Error>;
    async fn get_by_id(&self, id: i64) -> Result<Option<AudioArtifact>, sqlx::Error>;
    async fn get_by_artifact_no(&self, artifact_no: &str) -> Result<Option<AudioArtifact>, sqlx::Error>;
    async fn list_by_task(&self, task_id: i64) -> Result<Vec<AudioArtifact>, sqlx::Error>;
    async fn list(&self, filter: ArtifactFilter, limit: i64, offset: i64) -> Result<ArtifactListResult, sqlx::Error>;
    async fn count(&self, filter: ArtifactFilter) -> Result<i64, sqlx::Error>;
}