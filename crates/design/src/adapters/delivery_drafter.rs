//! 起草执行者桥接(feature = "agent"):把「起草设计稿」派发给 delivery 的 `AgentExecutor`
//! (=机群/本地 AI agent),以**架构师角色**(BMAD role,经 instructions 注入)产出设计稿。
//!
//! 异步:dispatch 把 WorkSpec 交给执行者即返回;agent 跑完应把设计稿
//! `POST /proposal/{id}/design` 回填(runtime 的 design 模式 bridge,见 plan TODO)。
//! 这里复用 delivery 的 WorkSpec/AgentExecutor 端口,故 design 仅在 `agent` feature 下依赖 delivery。

use async_trait::async_trait;
use std::sync::Arc;

use delivery::domain::ExecutorKind;
use delivery::ports::{AgentExecutor, NoopEventSink, WorkSpec};

use crate::domain::Proposal;
use crate::ports::{DesignDrafter, DraftError};

/// 架构师角色提示(BMAD role,可被更完整的 skill 组合替换)。
const ARCHITECT_ROLE: &str = "你是资深软件架构师。基于给定需求产出一份**可评审的 markdown 设计稿**,\
覆盖:方案概述、关键接口/数据模型、错误处理、风险与取舍。**不要写实现代码**。\
完成后把设计稿正文 POST 到 /proposal/{proposal_id}/design 回填(字段 doc)。";

pub struct DeliveryDesignDrafter {
    executor: Arc<dyn AgentExecutor>,
    kind: ExecutorKind,
}

impl DeliveryDesignDrafter {
    pub fn new(executor: Arc<dyn AgentExecutor>, kind: ExecutorKind) -> Self {
        Self { executor, kind }
    }
}

#[async_trait]
impl DesignDrafter for DeliveryDesignDrafter {
    async fn request_draft(&self, proposal: &Proposal) -> Result<(), DraftError> {
        let spec = WorkSpec {
            // 复用 proposal id 作关联标识,便于 agent 回调定位 /proposal/{id}/design。
            attempt_id: proposal.id.clone(),
            decomposition_id: proposal.requirement_id.clone(),
            task_id: proposal.id.clone(),
            title: format!("设计稿:{}", proposal.title),
            description: format!(
                "为需求 {} 起草设计方案(markdown)。proposal_id={}。",
                proposal.requirement_id, proposal.id
            ),
            acceptance_criteria: vec![
                "产出可评审的 markdown 设计稿".to_string(),
                "覆盖方案/接口/数据模型/错误处理/风险".to_string(),
            ],
            executor: self.kind,
            // 标记为设计模式:runtime 据此路由到 design bridge(产文档→POST /proposal/{id}/design),
            // 而非默认实现 bridge(改代码→commit→/delivery/{id}/complete)。
            context: Some("design".to_string()),
            instructions: Some(ARCHITECT_ROLE.replace("{proposal_id}", &proposal.id)),
        };
        self.executor
            .dispatch(&spec, &NoopEventSink)
            .await
            .map(|_| ())
            .map_err(|e| DraftError::Backend(e.to_string()))
    }
}
