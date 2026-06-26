use std::sync::Arc;

use kernel::page::{Page, PageRequest};

use crate::ports::{CaseExecutionQueryPort, CaseExecutionRecord, PortError};

#[derive(Clone)]
pub struct ListCaseExecutionsUseCase {
    query: Arc<dyn CaseExecutionQueryPort>,
}

impl ListCaseExecutionsUseCase {
    pub fn new(query: Arc<dyn CaseExecutionQueryPort>) -> Self {
        Self { query }
    }

    pub async fn execute(
        &self,
        case_id: &str,
        page: PageRequest,
    ) -> Result<Page<CaseExecutionRecord>, PortError> {
        let total = self.query.count_by_case(case_id).await?;
        let items = self.query.list_by_case(case_id, page.offset(), page.page_size()).await?;
        Ok(Page::of(page, total, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FakeQuery {
        records: Vec<CaseExecutionRecord>,
        last_window: Mutex<Option<(u64, u32)>>,
    }

    impl FakeQuery {
        fn new(n: u64) -> Self {
            let records = (0..n)
                .map(|i| CaseExecutionRecord {
                    report_id: format!("r{i}"),
                    case_id: "c1".into(),
                    outcome: "SUCCESS".into(),
                    failures: serde_json::json!([]),
                    executed_at: "2026-05-31T00:00:00Z".into(),
                })
                .collect();
            Self { records, last_window: Mutex::new(None) }
        }
    }

    #[async_trait]
    impl CaseExecutionQueryPort for FakeQuery {
        async fn count_by_case(&self, _case_id: &str) -> Result<u64, PortError> {
            Ok(self.records.len() as u64)
        }
        async fn list_by_case(
            &self,
            _case_id: &str,
            offset: u64,
            limit: u32,
        ) -> Result<Vec<CaseExecutionRecord>, PortError> {
            *self.last_window.lock().expect("lock") = Some((offset, limit));
            Ok(self
                .records
                .iter()
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn assembles_page_with_total_and_window() {
        let fake = Arc::new(FakeQuery::new(7));
        let uc = ListCaseExecutionsUseCase::new(fake.clone());
        let page = PageRequest::new(2, 3).expect("valid");
        let result = uc.execute("c1", page).await.expect("ok");
        assert_eq!(result.total, 7);
        assert_eq!(result.current, 2);
        assert_eq!(result.total_pages(), 3);
        assert_eq!(result.items.len(), 3);
        assert_eq!(*fake.last_window.lock().expect("lock"), Some((3, 3)));
    }

    #[tokio::test]
    async fn last_partial_page() {
        let fake = Arc::new(FakeQuery::new(7));
        let uc = ListCaseExecutionsUseCase::new(fake);
        let page = PageRequest::new(3, 3).expect("valid");
        let result = uc.execute("c1", page).await.expect("ok");
        assert_eq!(result.items.len(), 1);
    }
}
