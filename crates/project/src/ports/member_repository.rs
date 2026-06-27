use async_trait::async_trait;

use crate::domain::{NewMember, ProjectMember};
use crate::ports::RepoError;

#[async_trait]
pub trait ProjectMemberRepository: Send + Sync {
    /// 加入或改角色:同 (project_id, user_id) 已存在则更新 role。
    async fn upsert(&self, member: &NewMember) -> Result<ProjectMember, RepoError>;

    /// 按加入时间列出项目成员。
    async fn list(&self, project_id: &str) -> Result<Vec<ProjectMember>, RepoError>;

    /// 移除成员;返回是否确有一行被删。
    async fn remove(&self, project_id: &str, user_id: &str) -> Result<bool, RepoError>;
}
