//! 功能用例上下文:FunctionalCase 聚合(步骤 CaseStep + 预期结果)及其与需求的覆盖关联(CaseRequirement),
//! 为验收核查提供覆盖证据来源。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
