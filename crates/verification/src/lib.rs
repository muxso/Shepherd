//! 验收核查上下文:Verification 按验收准则逐条核对(CriterionStatus),关联覆盖证据(CoverageLink),
//! 产出 CriterionReport/CompletenessReport 与 Gap 清单;全部准则满足是需求进入 DELIVERED 的前提。
//! domain/application/ports 不做 IO;pg/http 适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
