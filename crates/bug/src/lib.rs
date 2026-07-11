//! 缺陷上下文:Bug 聚合 + 项目级状态流机(StatusFlowGraph,决定合法状态迁移)+
//! 关注人 + 与用例/需求等资产的关联(BugRelation)。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
