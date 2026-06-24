//! 内存版评论仓储(test double)。插入顺序即时间序;`created_at` 用单调序号造一个
//! 可排序的占位时间戳(测试无需真实墙钟)。

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
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let comment = Comment {
            id: format!("comment-{}", state.seq),
            target_type: new_comment.target_type.clone(),
            target_id: new_comment.target_id.clone(),
            content: new_comment.content.clone(),
            author: new_comment.author.clone(),
            // 零填充保证字典序 == 插入序(列出时按此排序)。
            created_at: format!("seq-{:020}", state.seq),
            deleted: false,
        };
        state.comments.insert(comment.id.clone(), comment.clone());
        Ok(comment)
    }

    async fn list(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>, RepoError> {
        let state = self.state.lock().expect("lock");
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
        let state = self.state.lock().expect("lock");
        Ok(state.comments.get(id).filter(|c| !c.deleted).cloned())
    }

    async fn soft_delete(&self, id: &str) -> Result<(), RepoError> {
        if let Some(c) = self.state.lock().expect("lock").comments.get_mut(id) {
            c.deleted = true;
        }
        Ok(())
    }
}
