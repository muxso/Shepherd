use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{Comment, NewComment};
use crate::ports::{CommentRepository, RepoError};

#[derive(Default)]
struct State {
    comments: HashMap<String, Comment>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryCommentRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryCommentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CommentRepository for InMemoryCommentRepository {
    async fn insert(&self, new_comment: &NewComment) -> Result<Comment, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let comment = Comment {
            id: format!("comment-{}", state.seq),
            target_type: new_comment.target_type.clone(),
            target_id: new_comment.target_id.clone(),
            content: new_comment.content.clone(),
            author: new_comment.author.clone(),
            // Zero-pad so lexical order == insertion order (list sorts on this).
            created_at: format!("seq-{:020}", state.seq),
            deleted: false,
        };
        state.comments.insert(comment.id.clone(), comment.clone());
        Ok(comment)
    }

    async fn list(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>, RepoError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out: Vec<Comment> = state
            .comments
            .values()
            .filter(|c| !c.deleted && c.target_type == target_type && c.target_id == target_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(out)
    }

    async fn get(&self, id: &str) -> Result<Option<Comment>, RepoError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(state.comments.get(id).filter(|c| !c.deleted).cloned())
    }

    async fn soft_delete(&self, id: &str) -> Result<(), RepoError> {
        if let Some(c) = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .comments
            .get_mut(id)
        {
            c.deleted = true;
        }
        Ok(())
    }
}
