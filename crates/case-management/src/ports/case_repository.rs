//! 功能用例仓储端口。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{FunctionalCase, NewFunctionalCase};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// 需求覆盖里的一条功能用例(JOIN ms_functional_case 带出名称/模块/优先级)。
#[derive(Debug, Clone)]
pub struct CoverageCase {
    pub criterion_index: i32,
    pub case_id: String,
    pub case_name: String,
    pub module: String,
    pub priority: String,
}

/// 一条功能用例覆盖的需求(JOIN ms_requirement 带出标题)。
#[derive(Debug, Clone)]
pub struct CaseRequirement {
    pub requirement_id: String,
    pub requirement_title: String,
    pub criterion_index: i32,
}

#[async_trait]
pub trait CaseRepository: Send + Sync {
    /// 插入用例,返回含 id 的视图。
    async fn insert(&self, c: &NewFunctionalCase) -> Result<FunctionalCase, RepoError>;

    /// 列出项目下用例(排除软删除)。
    async fn list_by_project(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError>;

    /// 按 id 读用例(排除软删除)。
    async fn get(&self, id: &str) -> Result<Option<FunctionalCase>, RepoError>;

    /// 关联功能用例到需求的某条验收标准(幂等)。
    async fn link_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
        project_id: &str,
    ) -> Result<(), RepoError>;

    /// 解除关联。
    async fn unlink_requirement_case(
        &self,
        requirement_id: &str,
        criterion_index: i32,
        functional_case_id: &str,
    ) -> Result<(), RepoError>;

    /// 列出覆盖某需求的功能用例(按标准下标分组用)。
    async fn cases_for_requirement(&self, requirement_id: &str) -> Result<Vec<CoverageCase>, RepoError>;

    /// 反查:某功能用例覆盖了哪些需求/标准。
    async fn requirements_for_case(&self, functional_case_id: &str) -> Result<Vec<CaseRequirement>, RepoError>;
}
