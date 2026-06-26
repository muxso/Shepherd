use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommentError {
    #[error("comment content must not be empty")]
    EmptyContent,
    #[error("comment target (type/id) must not be empty")]
    EmptyTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComment {
    pub target_type: String,
    pub target_id: String,
    pub content: String,
    pub author: String,
}

impl NewComment {
    pub fn new(
        target_type: &str,
        target_id: &str,
        content: &str,
        author: &str,
    ) -> Result<Self, CommentError> {
        let target_type = target_type.trim();
        let target_id = target_id.trim();
        if target_type.is_empty() || target_id.is_empty() {
            return Err(CommentError::EmptyTarget);
        }
        let content = content.trim();
        if content.is_empty() {
            return Err(CommentError::EmptyContent);
        }
        Ok(Self {
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            content: content.to_string(),
            author: author.trim().to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub content: String,
    pub author: String,
    pub created_at: String,
    pub deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_comment_trims_and_accepts_valid() {
        let c = NewComment::new("BUG", "b1", "  看起来是空指针  ", " admin ").expect("valid");
        assert_eq!(c.target_type, "BUG");
        assert_eq!(c.target_id, "b1");
        assert_eq!(c.content, "看起来是空指针");
        assert_eq!(c.author, "admin");
    }

    #[test]
    fn new_comment_rejects_blank_content() {
        assert_eq!(NewComment::new("BUG", "b1", "   ", "admin"), Err(CommentError::EmptyContent));
    }

    #[test]
    fn new_comment_rejects_blank_target() {
        assert_eq!(NewComment::new("  ", "b1", "x", "admin"), Err(CommentError::EmptyTarget));
        assert_eq!(NewComment::new("BUG", "", "x", "admin"), Err(CommentError::EmptyTarget));
    }
}
