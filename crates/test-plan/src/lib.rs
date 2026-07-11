//! 测试计划上下文:Plan 聚合(普通计划/计划组两种类型)圈选用例并跟踪执行结果,
//! 按 CaseStatus 汇总 CaseCounts 统计与报告;Schedule 定时调度产生 PlanRun 执行记录。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
