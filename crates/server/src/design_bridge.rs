//! 组装根桥接:design 的 `BreakdownTrigger` → task 的 `BreakdownUseCase`。
//! 设计稿审批通过 → 取该需求规格(基线版本)→ 自动拆分成任务 DAG(幂等)。

use async_trait::async_trait;

use design::domain::Proposal;
use design::ports::{BreakdownTrigger, TriggerError};
use requirement::application::RequirementService;
use task::application::{BreakdownError, BreakdownUseCase};
use task::ports::RequirementSpec;

pub struct ServerBreakdownTrigger {
    reqs: RequirementService,
    breakdown: BreakdownUseCase,
}

impl ServerBreakdownTrigger {
    pub fn new(reqs: RequirementService, breakdown: BreakdownUseCase) -> Self {
        Self { reqs, breakdown }
    }
}

#[async_trait]
impl BreakdownTrigger for ServerBreakdownTrigger {
    async fn on_design_approved(&self, p: &Proposal) -> Result<(), TriggerError> {
        let req = self
            .reqs
            .get(&p.requirement_id)
            .await
            .map_err(|e| TriggerError::Backend(format!("get requirement: {e:?}")))?;
        let version = req.baseline_version;
        let ver = req
            .version(version)
            .ok_or_else(|| TriggerError::Backend("requirement has no baseline version".into()))?;
        let spec = RequirementSpec {
            requirement_id: req.id.clone(),
            requirement_version: version,
            title: req.title.clone(),
            description: ver.description.clone(),
            acceptance_criteria: ver.acceptance_criteria.iter().map(|c| c.text.clone()).collect(),
        };
        match self.breakdown.execute(&spec).await {
            // 幂等:已拆分则视作成功(审批可重入)。
            Ok(_) | Err(BreakdownError::AlreadyExists) => Ok(()),
            Err(e) => Err(TriggerError::Backend(format!("breakdown: {e:?}"))),
        }
    }
}
