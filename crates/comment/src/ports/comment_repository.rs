//! 评论仓储端口:插入 / 按目标列出 / 取单条 / 软删。

use async_trait::async_trait;

use crate::domain::{Comment, NewComment};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait CommentRepository: Send + Sync {
    /// 落库一条评论,返回带 id/created_at 的完整记录。
    async fn insert(&self, new_comment: &NewComment) -> Result<Comment, RepoError>;

    /// 某目标下未删除的评论,按创建时间升序(老→新)。
    async fn list(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<Comment>, RepoError>;

    /// 软删除(置 deleted)。幂等。
    async fn soft_delete(&self, id: &str) -> Result<(), RepoError>;
}
