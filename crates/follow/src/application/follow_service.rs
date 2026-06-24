//! 关注服务:把入站参数过领域校验/归一化,再落到存储端口。
//!
//! 读路径(followers / following / is_following)也复用 `Follow::new` 做校验与
//! `entity_type` 归一化,保证写入与查询用同一把「小写化」尺子,不会查不到自己刚关注的。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::{Follow, FollowError};
use crate::ports::{FollowStore, RepoError};

#[derive(Debug, Error)]
pub enum FollowServiceError {
    #[error(transparent)]
    Validation(#[from] FollowError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct FollowService {
    store: Arc<dyn FollowStore>,
}

impl FollowService {
    pub fn new(store: Arc<dyn FollowStore>) -> Self {
        Self { store }
    }

    /// 关注(幂等)。返回 `true` 表示本次新建。
    pub async fn follow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, FollowServiceError> {
        let f = Follow::new(project_id, entity_type, entity_id, user_id)?;
        Ok(self.store.follow(&f).await?)
    }

    /// 取消关注(幂等)。返回 `true` 表示确实移除。
    pub async fn unfollow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, FollowServiceError> {
        let f = Follow::new(project_id, entity_type, entity_id, user_id)?;
        Ok(self.store.unfollow(&f.project_id, &f.entity_type, &f.entity_id, &f.user_id).await?)
    }

    /// 某对象的关注人列表。
    pub async fn followers(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<String>, FollowServiceError> {
        // 借 `Follow::new` 校验 + 归一化 entity_type;user 占位非空。
        let f = Follow::new(project_id, entity_type, entity_id, "-")?;
        Ok(self.store.followers(&f.project_id, &f.entity_type, &f.entity_id).await?)
    }

    /// 当前用户是否关注了某对象。
    pub async fn is_following(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, FollowServiceError> {
        let f = Follow::new(project_id, entity_type, entity_id, user_id)?;
        let people = self.store.followers(&f.project_id, &f.entity_type, &f.entity_id).await?;
        Ok(people.iter().any(|u| u == &f.user_id))
    }

    /// 当前用户在某项目下关注的对象 id(可按类型过滤)。
    pub async fn following_ids(
        &self,
        project_id: &str,
        user_id: &str,
        entity_type: Option<&str>,
    ) -> Result<Vec<String>, FollowServiceError> {
        let project_id = project_id.trim();
        let user_id = user_id.trim();
        if project_id.is_empty() {
            return Err(FollowError::EmptyProject.into());
        }
        if user_id.is_empty() {
            return Err(FollowError::EmptyUser.into());
        }
        let norm = entity_type.map(|t| t.trim().to_ascii_lowercase());
        Ok(self.store.following_ids(project_id, user_id, norm.as_deref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryFollowStore;

    fn svc() -> FollowService {
        FollowService::new(Arc::new(InMemoryFollowStore::new()))
    }

    #[tokio::test]
    async fn follow_is_idempotent_and_listed() {
        let s = svc();
        assert!(s.follow("p1", "Bug", "b1", "alice").await.expect("ok")); // 新建
        assert!(!s.follow("p1", "bug", "b1", "alice").await.expect("ok")); // 大小写归一 → 已存在
        assert!(s.is_following("p1", "BUG", "b1", "alice").await.expect("ok"));
        assert_eq!(s.followers("p1", "bug", "b1").await.expect("ok"), vec!["alice".to_string()]);
        assert_eq!(s.following_ids("p1", "alice", Some("bug")).await.expect("ok"), vec!["b1".to_string()]);
    }

    #[tokio::test]
    async fn unfollow_removes_and_is_idempotent() {
        let s = svc();
        s.follow("p1", "bug", "b1", "alice").await.expect("ok");
        assert!(s.unfollow("p1", "bug", "b1", "alice").await.expect("ok")); // 真移除
        assert!(!s.unfollow("p1", "bug", "b1", "alice").await.expect("ok")); // 再取消无副作用
        assert!(!s.is_following("p1", "bug", "b1", "alice").await.expect("ok"));
        assert!(s.followers("p1", "bug", "b1").await.expect("ok").is_empty());
    }

    #[tokio::test]
    async fn following_ids_filters_by_type() {
        let s = svc();
        s.follow("p1", "bug", "b1", "alice").await.expect("ok");
        s.follow("p1", "requirement", "r1", "alice").await.expect("ok");
        s.follow("p1", "bug", "b2", "bob").await.expect("ok"); // 别人,不计
        let mut bugs = s.following_ids("p1", "alice", Some("bug")).await.expect("ok");
        bugs.sort();
        assert_eq!(bugs, vec!["b1".to_string()]);
        let mut all = s.following_ids("p1", "alice", None).await.expect("ok");
        all.sort();
        assert_eq!(all, vec!["b1".to_string(), "r1".to_string()]);
    }

    #[tokio::test]
    async fn rejects_blank_input() {
        let s = svc();
        assert!(matches!(
            s.follow("p1", "bug", "", "alice").await,
            Err(FollowServiceError::Validation(FollowError::EmptyEntityId))
        ));
    }
}
