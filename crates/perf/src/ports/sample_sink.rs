//! 样本下沉端口:把一轮压测的**原始逐请求样本**写到冷/分析存储(Parquet+S3 等)。
//!
//! 聚合摘要(LoadReport)进 PG 供轮询/列表;原始样本量大、只写不改、按列分析,
//! 更适合列存对象存储。该端口让"写到哪"可换(内存 / Parquet+对象存储),引擎不感知。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::Sample;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SinkError {
    #[error("sample sink error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait SampleSink: Send + Sync {
    /// 写入某次压测的全部样本,返回可定位的存储键(如对象路径),供 PG 报告行引用。
    async fn write(&self, run_id: &str, samples: &[Sample]) -> Result<String, SinkError>;
}
