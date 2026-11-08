use case_management::{
    domain::test_assets::{AssetError, AssetResult, SourceSnapshot},
    ports::test_assets::AssetSource,
};
#[cfg(feature = "yunxiao")]
use serde_json::Value;
use std::sync::Arc;
pub async fn reader() -> Result<Option<Arc<dyn AssetSource>>, Box<dyn std::error::Error>> {
    #[cfg(feature = "yunxiao")]
    if crate::plugins::yunxiao_enabled() {
        let url = std::env::var("SHEPHERD_YUNXIAO_DATABASE_URL")?;
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&url).await?;
        return Ok(Some(Arc::new(YunxiaoSource(Arc::new(yunxiao::adapters::pg::PgStore(pool))))));
    }
    Ok(None)
}
#[cfg(feature = "yunxiao")]
struct YunxiaoSource(Arc<dyn yunxiao::ports::Store>);
#[cfg(feature = "yunxiao")]
#[async_trait::async_trait]
impl AssetSource for YunxiaoSource {
    async fn linked_changes(&self, source: &str, external: &str) -> AssetResult<Value> {
        let mut items = Vec::new();
        let mut expected = None;
        for page in 1..=100 {
            let value = self
                .0
                .requirement_output(yunxiao::domain::RequirementOutputQuery {
                    source_id: Some(source.into()),
                    reference: external.into(),
                    commit_page: Some(page),
                    page_size: Some(100),
                    ..Default::default()
                })
                .await
                .map_err(|_| AssetError::Storage)?;
            let output = &value["output"];
            let total = output["commitTotal"].as_u64().unwrap_or(0);
            if expected.is_some_and(|n| n != total) {
                return Err(AssetError::Conflict("code associations changed while loading".into()));
            }
            expected = Some(total);
            let rows = output["commits"].as_array().ok_or(AssetError::Storage)?;
            items.extend(rows.iter().cloned());
            if items.len() as u64 >= total {
                return Ok(serde_json::json!({"items":items,"available":true}));
            }
            if rows.is_empty() {
                return Err(AssetError::Conflict("code association pagination incomplete".into()));
            }
        }
        Err(AssetError::Conflict("too many linked commits for one review".into()))
    }
    async fn sources(&self) -> AssetResult<Value> {
        self.0.sources().await.map_err(|_| AssetError::Storage)
    }
    async fn snapshot(&self, id: &str) -> AssetResult<SourceSnapshot> {
        let value = self.0.asset_export(id).await.map_err(|e| match e {
            yunxiao::domain::Error::NotFound => AssetError::NotFound,
            _ => AssetError::Storage,
        })?;
        let snapshot: SourceSnapshot =
            serde_json::from_value(value).map_err(|_| AssetError::Storage)?;
        if snapshot.source_id != id || snapshot.revision.is_empty() {
            return Err(AssetError::Conflict("source snapshot is incomplete".into()));
        }
        Ok(snapshot)
    }
}

struct Changes(Arc<dyn AssetSource>);
#[async_trait::async_trait]
impl rag::ports::ChangeSource for Changes {
    async fn linked_changes(
        &self,
        source: &str,
        external: &str,
    ) -> rag::domain::Result<serde_json::Value> {
        self.0
            .linked_changes(source, external)
            .await
            .map_err(|_| rag::domain::RagError::Backend("linked changes unavailable".into()))
    }
}
pub fn changes(source: Option<Arc<dyn AssetSource>>) -> Option<Arc<dyn rag::ports::ChangeSource>> {
    source.map(|s| Arc::new(Changes(s)) as Arc<dyn rag::ports::ChangeSource>)
}
