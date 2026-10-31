use crate::{
    domain::{Error, Record},
    ports::Store,
};
use serde_json::Value;
use std::sync::Arc;
#[derive(Clone)]
pub struct Service(pub Arc<dyn Store>);
impl Service {
    pub async fn stage(&self, run: &str, records: Vec<Record>) -> Result<Value, Error> {
        if records.is_empty() || records.len() > 200 {
            return Err(Error::Invalid("batch must contain 1..200 records".into()));
        }
        for r in &records {
            r.validate()?;
        }
        self.0.stage(run, records).await
    }
}
