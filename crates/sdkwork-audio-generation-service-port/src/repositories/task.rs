//! Task repository port

use async_trait::async_trait;

use crate::entities::task::*;

/// Task repository port trait
#[async_trait]
pub trait TaskRepository {
    async fn create(&self, request: CreateTaskRequest) -> Result<AudioGenerationTask, sqlx::Error>;
    async fn get_by_id(&self, id: i64) -> Result<Option<AudioGenerationTask>, sqlx::Error>;
    async fn get_by_task_no(&self, task_no: &str) -> Result<Option<AudioGenerationTask>, sqlx::Error>;
    async fn get_by_idempotency_key(
        &self,
        tenant_id: i64,
        operation_type: &str,
        idempotency_key: &str,
    ) -> Result<Option<AudioGenerationTask>, sqlx::Error>;
    async fn update(&self, id: i64, request: UpdateTaskRequest) -> Result<AudioGenerationTask, sqlx::Error>;
    async fn list(&self, filter: TaskFilter, limit: i64, offset: i64) -> Result<TaskListResult, sqlx::Error>;
    async fn count(&self, filter: TaskFilter) -> Result<i64, sqlx::Error>;
}