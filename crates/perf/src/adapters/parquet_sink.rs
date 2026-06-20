//! Parquet + 对象存储样本下沉:把逐请求样本写成一个 Parquet 对象。
//!
//! schema:`latency_ms: u64, success: bool`(后续可加 ts_offset_ms / status_code)。
//! 后端经 `object_store` 抽象:内存(测试)、本地文件系统(开发/单机)、S3/GCS/Azure(生产,
//! 加对应 feature)。键形如 `{prefix}/run_id=<id>/part-0.parquet`,便于按 run 分区与跨 run 扫描。
//! 写 UNCOMPRESSED(纯 Rust,无 C 压缩依赖);量大可改 row-group 分块流式 + 压缩编码。

use std::sync::Arc;

use arrow::array::{BooleanArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjPath;
use object_store::ObjectStore;
use parquet::arrow::ArrowWriter;

use crate::domain::Sample;
use crate::ports::{SampleSink, SinkError};

fn be<E: std::fmt::Display>(e: E) -> SinkError {
    SinkError::Backend(e.to_string())
}

pub struct ParquetObjectStoreSink {
    store: Arc<dyn ObjectStore>,
    prefix: String,
}

impl ParquetObjectStoreSink {
    /// 用任意 object_store 后端构造(测试用 `InMemory`,生产用 S3 等)。
    pub fn new(store: Arc<dyn ObjectStore>, prefix: impl Into<String>) -> Self {
        Self { store, prefix: prefix.into() }
    }

    /// 本地文件系统后端(开发/单机);`root` 须为已存在目录。
    pub fn new_local(
        root: impl AsRef<std::path::Path>,
        prefix: impl Into<String>,
    ) -> Result<Self, SinkError> {
        let fs = LocalFileSystem::new_with_prefix(root).map_err(be)?;
        Ok(Self::new(Arc::new(fs), prefix))
    }

    /// 样本 → Parquet 字节(Arrow RecordBatch → ArrowWriter)。
    fn encode(samples: &[Sample]) -> Result<Vec<u8>, SinkError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("latency_ms", DataType::UInt64, false),
            Field::new("success", DataType::Boolean, false),
        ]));
        let latency = UInt64Array::from(samples.iter().map(|s| s.latency_ms).collect::<Vec<_>>());
        let success = BooleanArray::from(samples.iter().map(|s| s.success).collect::<Vec<_>>());
        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(latency), Arc::new(success)])
            .map_err(be)?;

        let mut buf: Vec<u8> = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buf, schema, None).map_err(be)?;
        writer.write(&batch).map_err(be)?;
        writer.close().map_err(be)?;
        Ok(buf)
    }

    fn object_key(&self, run_id: &str) -> String {
        format!("{}/run_id={run_id}/part-0.parquet", self.prefix.trim_end_matches('/'))
    }
}

#[async_trait]
impl SampleSink for ParquetObjectStoreSink {
    async fn write(&self, run_id: &str, samples: &[Sample]) -> Result<String, SinkError> {
        let bytes = Self::encode(samples)?;
        let key = self.object_key(run_id);
        self.store.put(&ObjPath::from(key.clone()), bytes.into()).await.map_err(be)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Array, BooleanArray, UInt64Array};
    use object_store::memory::InMemory;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    #[tokio::test]
    async fn writes_real_parquet_readable_back() {
        let store = Arc::new(InMemory::new());
        let sink = ParquetObjectStoreSink::new(store.clone(), "perf");
        let samples = vec![
            Sample::new(10, true),
            Sample::new(20, true),
            Sample::new(900, false),
            Sample::new(15, true),
        ];

        let key = sink.write("run-1", &samples).await.expect("write");
        assert_eq!(key, "perf/run_id=run-1/part-0.parquet");

        // 从对象存储取回字节,用 parquet reader 解析,验证行数与列值。
        let bytes = store
            .get(&ObjPath::from(key))
            .await
            .expect("get")
            .bytes()
            .await
            .expect("bytes");
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .expect("builder")
            .build()
            .expect("reader");

        let mut rows = 0usize;
        let mut failures = 0usize;
        let mut max_latency = 0u64;
        for batch in reader {
            let batch = batch.expect("batch");
            rows += batch.num_rows();
            let lat = batch
                .column(0)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("u64 col");
            let suc = batch
                .column(1)
                .as_any()
                .downcast_ref::<BooleanArray>()
                .expect("bool col");
            for i in 0..batch.num_rows() {
                max_latency = max_latency.max(lat.value(i));
                if !suc.value(i) {
                    failures += 1;
                }
            }
        }
        assert_eq!(rows, 4);
        assert_eq!(failures, 1);
        assert_eq!(max_latency, 900);
    }
}
