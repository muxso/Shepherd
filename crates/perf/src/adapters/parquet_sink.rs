// Writes UNCOMPRESSED to avoid pulling in C compression deps and stay pure Rust.

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
    pub fn new(store: Arc<dyn ObjectStore>, prefix: impl Into<String>) -> Self {
        Self { store, prefix: prefix.into() }
    }

    pub fn new_local(
        root: impl AsRef<std::path::Path>,
        prefix: impl Into<String>,
    ) -> Result<Self, SinkError> {
        let fs = LocalFileSystem::new_with_prefix(root).map_err(be)?;
        Ok(Self::new(Arc::new(fs), prefix))
    }

    fn encode(samples: &[Sample]) -> Result<Vec<u8>, SinkError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("latency_ms", DataType::UInt64, false),
            Field::new("success", DataType::Boolean, false),
        ]));
        let latency = UInt64Array::from(samples.iter().map(|s| s.latency_ms).collect::<Vec<_>>());
        let success = BooleanArray::from(samples.iter().map(|s| s.success).collect::<Vec<_>>());
        let batch =
            RecordBatch::try_new(schema.clone(), vec![Arc::new(latency), Arc::new(success)])
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

        let bytes =
            store.get(&ObjPath::from(key)).await.expect("get").bytes().await.expect("bytes");
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
            let lat = batch.column(0).as_any().downcast_ref::<UInt64Array>().expect("u64 col");
            let suc = batch.column(1).as_any().downcast_ref::<BooleanArray>().expect("bool col");
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
