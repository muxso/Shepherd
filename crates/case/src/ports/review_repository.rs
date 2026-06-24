//! 评审仓储端口。聚合所需的最小依赖:配置、历史、追加、回写状态。

use async_trait::async_trait;

use crate::domain::{ReviewRecord, ReviewSetting, ReviewStatus};

use thiserror::Error;

/// 评审列表项(队列概览):规则 + 用例总数 + 已通过数 + 创建时间 + 整体状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSummary {
    pub id: String,
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub total: usize,
    pub passed: usize,
    pub created_at: String,
    /// 整体生命周期:IN_PROGRESS(进行中)/ COMPLETED(已完成)。
    pub status: String,
}

/// 评审里某条用例的聚合状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewCaseStatus {
    pub case_id: String,
    pub status: String,
}

/// 评审详情:配置 + 每条用例的当前状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewDetail {
    pub id: String,
    pub pass_rule: String,
    pub reviewer_count: usize,
    pub cases: Vec<ReviewCaseStatus>,
}

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

    // —— 队列管理(默认实现:测试用的内存仓储无需支持;PG 覆盖)。——

    /// 新建评审:指定项目 / 通过规则 / 评审人数 / 用例集,返回 review_id。
    async fn create_review(
        &self,
        _project_id: &str,
        _pass_rule: &str,
        _reviewer_count: usize,
        _case_ids: &[String],
    ) -> Result<String, RepoError> {
        Err(RepoError::Backend("create_review unsupported".into()))
    }

    /// 列出某项目下的评审(队列概览)。
    async fn list_reviews(&self, _project_id: &str) -> Result<Vec<ReviewSummary>, RepoError> {
        Ok(Vec::new())
    }

    /// 取一个评审的详情(配置 + 每条用例状态)。
    async fn get_review(&self, _review_id: &str) -> Result<ReviewDetail, RepoError> {
        Err(RepoError::NotFound)
    }
}
