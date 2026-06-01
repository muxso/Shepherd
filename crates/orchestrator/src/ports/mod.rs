//! 编排所需的 gateway 端口(由组装根接到 task / verification 的真实服务)。

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrchError {
    #[error("gateway error: {0}")]
    Gateway(String),
}

/// 任务推进目标(编排器自有,组装根映射到 task 的状态)。Dispatched/Delivered 由推进过程隐含。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTarget {
    Running,
    /// 已交付(过验证门前的中间态)。
    Delivered,
    /// 交付成功并通过验证门 → 任务验证通过(解锁依赖它的下游任务)。
    Verified,
    Failed,
}

/// task 上下文侧:定位需求版本 + 推进任务生命周期 + 取任务验收标准。
#[async_trait]
pub trait TaskGateway: Send + Sync {
    /// 拆分图归属的 (requirement_id, version);不存在返回 None。
    async fn requirement_of(
        &self,
        decomposition_id: &str,
    ) -> Result<Option<(String, u32)>, OrchError>;

    /// 把任务推进到 target(实现侧沿 happy path 走、幂等)。
    async fn advance_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
        target: TaskTarget,
    ) -> Result<(), OrchError>;

    /// 取任务的验收标准(交给 judge 评判交付物)。
    async fn task_criteria(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<String>, OrchError>;
}

/// 交付物视图(编排器自有,与 delivery 的类型解耦)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliverableView {
    pub kind: String,
    pub reference: String,
    pub summary: String,
}

/// 验证门裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub passed: bool,
    pub reason: String,
}

/// 验证门(judge):据验收标准评判交付物是否达标。可由规则或 LLM 实现。
#[async_trait]
pub trait Judge: Send + Sync {
    async fn judge(&self, criteria: &[String], deliverable: &DeliverableView) -> Verdict;
}

/// 验证侧:查某需求版本的验证 + 同步任务的覆盖链状态。
#[async_trait]
pub trait VerificationGateway: Send + Sync {
    /// 某需求版本的验证 id;未开验证返回 None。
    async fn find_verification(
        &self,
        requirement_id: &str,
        version: u32,
    ) -> Result<Option<String>, OrchError>;

    /// 把某任务的交付验证状态同步进验证覆盖链。
    async fn sync(
        &self,
        verification_id: &str,
        decomposition_id: &str,
        task_id: &str,
        satisfied: bool,
    ) -> Result<(), OrchError>;
}
