//! 设计起草出站端口:把「为某提案起草设计稿」交给某种执行者(AI agent / 人工占位)。
//!
//! 异步语义:`request_draft` 只负责**派发**;真正的设计稿由执行者跑完后经
//! `POST /proposal/{id}/design`(→ `submit_design`)回填,进入待审。
//! 这样 design 上下文不与具体执行后端耦合(delivery / fleet runtime 由组装根桥接)。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Proposal;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DraftError {
    #[error("drafter backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait DesignDrafter: Send + Sync {
    /// 请求为该提案起草设计稿(异步派发,跑完经 submit_design 回填)。
    async fn request_draft(&self, proposal: &Proposal) -> Result<(), DraftError>;
}
