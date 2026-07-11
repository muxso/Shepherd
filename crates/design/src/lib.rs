//! 设计稿上下文:需求拆分前的设计与人审批门。Proposal 状态机
//! DRAFTING → PENDING_REVIEW → APPROVED / CHANGES_REQUESTED(修订循环),APPROVED 为唯一终态。
//! DesignDrafter 端口生成设计稿,批准后经 BreakdownTrigger 触发任务拆分。
//! domain/application/ports 不做 IO;pg/http/agent 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
