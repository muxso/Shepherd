use async_trait::async_trait;
use std::sync::Arc;

use delivery::domain::ExecutorKind;
use delivery::ports::{AgentExecutor, NoopEventSink, WorkSpec};

use crate::domain::Proposal;
use crate::ports::{DesignDrafter, DraftError};

const ARCHITECT_ROLE: &str = "你是资深软件架构师。基于给定需求产出一份**可评审的 markdown 设计稿**,\
覆盖:方案概述、关键接口/数据模型、错误处理、风险与取舍。**不要写实现代码**。\
**直接把设计稿 markdown 作为你的最终回答正文输出**——不要写文件、不要发起网络请求、\
不要尝试调用任何接口;系统会自动采集你的输出并回填。";

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
            // "design" 路由 runtime 到 design bridge(回填 /proposal/{id}/design),而非默认实现 bridge。
            context: Some("design".to_string()),
            instructions: Some(ARCHITECT_ROLE.replace("{proposal_id}", &proposal.id)),
            target_runtime: None,
        };
        self.executor
            .dispatch(&spec, &NoopEventSink)
            .await
            .map(|_| ())
            .map_err(|e| DraftError::Backend(e.to_string()))
    }
}
