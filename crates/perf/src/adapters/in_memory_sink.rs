use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::Sample;
use crate::ports::{SampleSink, SinkError};

#[derive(Default)]
pub struct InMemorySampleSink {
    writes: Mutex<Vec<(String, Vec<Sample>)>>,
}

impl InMemorySampleSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn writes(&self) -> Vec<(String, Vec<Sample>)> {
        self.writes.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl SampleSink for InMemorySampleSink {
    async fn write(&self, run_id: &str, samples: &[Sample]) -> Result<String, SinkError> {
        self.writes
            .lock()
            .map_err(|e| SinkError::Backend(e.to_string()))?
            .push((run_id.to_string(), samples.to_vec()));
        Ok(format!("memory://{run_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_writes_and_returns_key() {
        let sink = InMemorySampleSink::new();
        let key =
            sink.write("r1", &[Sample::new(5, true), Sample::new(9, false)]).await.expect("ok");
        assert_eq!(key, "memory://r1");
        let w = sink.writes();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].0, "r1");
        assert_eq!(w[0].1.len(), 2);
    }
}
