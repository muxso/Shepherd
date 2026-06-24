//! 关注关系存储端口。
//!
//! 全部以归一化后的四元组寻址。`follow`/`unfollow` 幂等并回报「是否真的改变了状态」,
//! 让上层能区分「新关注」与「本来就关注着」(便于前端提示与计数)。

use async_trait::async_trait;

use crate::domain::Follow;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait FollowStore: Send + Sync {
    /// 幂等关注;返回 `true` 表示本次新建(原本未关注),`false` 表示已存在。
    async fn follow(&self, f: &Follow) -> Result<bool, RepoError>;

    /// 幂等取消关注;返回 `true` 表示确实移除了一条,`false` 表示原本就没关注。
    async fn unfollow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, RepoError>;

    /// 某对象的关注人(user_id 列表,按关注时间升序)。
    async fn followers(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<String>, RepoError>;

    /// 某用户在某项目下关注的对象 id 列表(可按 `entity_type` 过滤,None=全部类型)。
    async fn following_ids(
        &self,
        project_id: &str,
        user_id: &str,
        entity_type: Option<&str>,
    ) -> Result<Vec<String>, RepoError>;
}
