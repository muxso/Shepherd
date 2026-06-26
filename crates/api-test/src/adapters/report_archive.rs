use std::sync::Arc;

use arrow::array::{Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjPath;
use object_store::ObjectStore;
use parquet::arrow::ArrowWriter;

use super::pg::BatchReportDetail;
use crate::ports::PortError;

fn be<E: std::fmt::Display>(e: E) -> PortError {
    PortError::Backend(e.to_string())
}

pub struct ReportArchiver {
    store: Arc<dyn ObjectStore>,
    prefix: String,
}

impl ReportArchiver {
    pub fn new(store: Arc<dyn ObjectStore>, prefix: impl Into<String>) -> Self {
        Self { store, prefix: prefix.into() }
    }

    pub fn new_local(root: impl AsRef<std::path::Path>, prefix: impl Into<String>) -> Result<Self, PortError> {
        let fs = LocalFileSystem::new_with_prefix(root).map_err(be)?;
        Ok(Self::new(Arc::new(fs), prefix))
    }

    fn encode(report_id: &str, d: &BatchReportDetail) -> Result<Vec<u8>, PortError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("report_id", DataType::Utf8, false),
            Field::new("report_status", DataType::Utf8, false),
            Field::new("case_count", DataType::Int32, false),
            Field::new("case_id", DataType::Utf8, false),
            Field::new("outcome", DataType::Utf8, false),
            Field::new("status_code", DataType::Int32, true),
            Field::new("latency_ms", DataType::Int64, true),
            Field::new("resp_size", DataType::Int64, true),
            Field::new("executed_at", DataType::Utf8, false),
            Field::new("failures", DataType::Utf8, false),
            Field::new("assertions", DataType::Utf8, false),
            Field::new("extractions", DataType::Utf8, false),
            Field::new("req_method", DataType::Utf8, true),
            Field::new("req_url", DataType::Utf8, true),
            Field::new("req_headers", DataType::Utf8, false),
            Field::new("req_body", DataType::Utf8, true),
        ]));
        let n = d.results.len();
        let report_ids = StringArray::from(vec![report_id; n]);
        let report_status = StringArray::from(vec![d.status.as_str(); n]);
        let case_count = Int32Array::from(vec![d.case_count; n]);
        let case_id = StringArray::from(d.results.iter().map(|r| r.case_id.as_str()).collect::<Vec<_>>());
        let outcome = StringArray::from(d.results.iter().map(|r| r.outcome.as_str()).collect::<Vec<_>>());
        let status_code = Int32Array::from(d.results.iter().map(|r| r.status_code).collect::<Vec<_>>());
        let latency_ms = Int64Array::from(d.results.iter().map(|r| r.latency_ms).collect::<Vec<_>>());
        let resp_size = Int64Array::from(d.results.iter().map(|r| r.resp_size).collect::<Vec<_>>());
        let executed_at = StringArray::from(d.results.iter().map(|r| r.executed_at.as_str()).collect::<Vec<_>>());
        let failures = StringArray::from(
            d.results.iter().map(|r| serde_json::json!(r.failures).to_string()).collect::<Vec<_>>(),
        );
        let assertions = StringArray::from(d.results.iter().map(|r| r.assertions.to_string()).collect::<Vec<_>>());
        let extractions = StringArray::from(d.results.iter().map(|r| r.extractions.to_string()).collect::<Vec<_>>());
        let req_method = StringArray::from(d.results.iter().map(|r| r.req_method.as_deref()).collect::<Vec<_>>());
        let req_url = StringArray::from(d.results.iter().map(|r| r.req_url.as_deref()).collect::<Vec<_>>());
        let req_headers = StringArray::from(
            d.results.iter().map(|r| serde_json::json!(r.req_headers).to_string()).collect::<Vec<_>>(),
        );
        let req_body = StringArray::from(d.results.iter().map(|r| r.req_body.as_deref()).collect::<Vec<_>>());
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(report_ids),
                Arc::new(report_status),
                Arc::new(case_count),
                Arc::new(case_id),
                Arc::new(outcome),
                Arc::new(status_code),
                Arc::new(latency_ms),
                Arc::new(resp_size),
                Arc::new(executed_at),
                Arc::new(failures),
                Arc::new(assertions),
                Arc::new(extractions),
                Arc::new(req_method),
                Arc::new(req_url),
                Arc::new(req_headers),
                Arc::new(req_body),
            ],
        )
        .map_err(be)?;

        let mut buf: Vec<u8> = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buf, schema, None).map_err(be)?;
        writer.write(&batch).map_err(be)?;
        writer.close().map_err(be)?;
        Ok(buf)
    }

    fn object_key(&self, report_id: &str, d: &BatchReportDetail) -> String {
        let dt = d
            .results
            .first()
            .map(|r| r.executed_at.chars().take(10).collect::<String>())
            .filter(|s| s.len() == 10)
            .unwrap_or_else(|| "unknown".to_string());
        format!("{}/dt={dt}/report_id={report_id}/part-0.parquet", self.prefix.trim_end_matches('/'))
    }

    pub async fn archive(&self, report_id: &str, d: &BatchReportDetail) -> Result<String, PortError> {
        let bytes = Self::encode(report_id, d)?;
        let key = self.object_key(report_id, d);
        self.store.put(&ObjPath::from(key.clone()), bytes.into()).await.map_err(be)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::pg::CaseResultRow;
    use object_store::memory::InMemory;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    fn row(case_id: &str, outcome: &str, status: Option<i32>) -> CaseResultRow {
        CaseResultRow {
            case_id: case_id.to_string(),
            outcome: outcome.to_string(),
            failures: vec![],
            executed_at: "2026-06-23T10:00:00+00:00".to_string(),
            status_code: status,
            latency_ms: Some(12),
            resp_size: Some(34),
            body: Some("ok".to_string()),
            headers: vec![],
            assertions: serde_json::json!([{"item": "状态码", "passed": true}]),
            extractions: serde_json::json!([["token", "abc"]]),
            req_method: Some("GET".to_string()),
            req_url: Some("http://x/healthz".to_string()),
            req_headers: vec![],
            req_body: None,
        }
    }

    #[tokio::test]
    async fn archives_real_parquet_readable_back() {
        let store = Arc::new(InMemory::new());
        let arch = ReportArchiver::new(store.clone(), "reports");
        let detail = BatchReportDetail {
            status: "ERROR".to_string(),
            case_count: 2,
            started_at: None,
            finished_at: None,
            duration_ms: None,
            results: vec![row("c1", "SUCCESS", Some(200)), row("c2", "ERROR", Some(404))],
        };

        let key = arch.archive("rep-1", &detail).await.expect("archive");
        assert_eq!(key, "reports/dt=2026-06-23/report_id=rep-1/part-0.parquet");

        let bytes = store
            .get(&ObjPath::from(key))
            .await
            .expect("get")
            .bytes()
            .await
            .expect("bytes");
        let reader =
            ParquetRecordBatchReaderBuilder::try_new(bytes).expect("builder").build().expect("reader");
        let mut rows = 0usize;
        let mut errors = 0usize;
        for batch in reader {
            let batch = batch.expect("batch");
            rows += batch.num_rows();
            let outcome =
                batch.column(4).as_any().downcast_ref::<StringArray>().expect("outcome col");
            let req_method =
                batch.column(12).as_any().downcast_ref::<StringArray>().expect("req_method col");
            for i in 0..batch.num_rows() {
                if outcome.value(i) == "ERROR" {
                    errors += 1;
                }
                assert_eq!(req_method.value(i), "GET");
            }
        }
        assert_eq!(rows, 2);
        assert_eq!(errors, 1);
    }
}
