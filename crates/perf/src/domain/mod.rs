//! 领域层:负载计划 + 样本统计聚合(零 IO 纯函数)。
pub mod load;

pub use load::{aggregate, LatencyStats, LoadError, LoadMode, LoadPlan, LoadReport, Sample};
