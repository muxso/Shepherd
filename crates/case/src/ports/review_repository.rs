//! 评审仓储端口。聚合所需的最小依赖:配置、历史、追加、回写状态。

use async_trait::async_trait;

use crate::domain::{ReviewRecord, ReviewSetting, ReviewStatus};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
    /// 指定评审/用例不存在。
    #[error("review or case not found")]
    NotFound,
}

#[async_trait]
pub trait ReviewRepository: Send + Sync {
    /// 评审配置(通过规则 + 评审人数)。
    async fn review_setting(&self, review_id: &str) -> Result<ReviewSetting, RepoError>;

    /// 某用例在某评审下的历史(按时间升序)。
    async fn history_of(
        &self,
        review_id: &str,
        case_id: &str,
    ) -> Result<Vec<ReviewRecord>, RepoError>;

    /// 追加一条评审历史。
    async fn append_history(
        &self,
        review_id: &str,
        case_id: &str,
        record: &ReviewRecord,
    ) -> Result<(), RepoError>;

    /// 回写聚合后的用例评审状态。
    async fn set_case_status(
        &self,
        review_id: &str,
        case_id: &str,
        status: ReviewStatus,
    ) -> Result<(), RepoError>;
}
