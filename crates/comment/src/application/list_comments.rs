use std::sync::Arc;

use crate::domain::Comment;
use crate::ports::{CommentRepository, RepoError};

#[derive(Clone)]
pub struct ListCommentsUseCase {
    repo: Arc<dyn CommentRepository>,
}

impl ListCommentsUseCase {
    pub fn new(repo: Arc<dyn CommentRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Vec<Comment>, RepoError> {
        self.repo.list(target_type, target_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryCommentRepository;
    use crate::application::AddCommentUseCase;

    #[tokio::test]
    async fn lists_only_matching_target_in_order() {
        let repo = Arc::new(InMemoryCommentRepository::new());
        let add = AddCommentUseCase::new(repo.clone());
        add.execute("BUG", "b1", "first", "a").await.expect("ok");
        add.execute("BUG", "b1", "second", "b").await.expect("ok");
        add.execute("BUG", "b2", "other", "c").await.expect("ok");

        let list = ListCommentsUseCase::new(repo).execute("BUG", "b1").await.expect("ok");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].content, "first");
        assert_eq!(list[1].content, "second");
    }
}
