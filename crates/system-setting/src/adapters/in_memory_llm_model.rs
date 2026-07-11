use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use crate::ports::{LlmModelPatch, LlmModelRecord, LlmModelRepoError, LlmModelRepository};

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 测试/本地用的内存实现;BTreeMap 保证 list 顺序稳定,id 用递增序号。
#[derive(Default)]
struct LlmModelState {
    rows: BTreeMap<String, LlmModelRecord>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryLlmModelRepository {
    state: Arc<Mutex<LlmModelState>>,
}

impl InMemoryLlmModelRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

fn dup(rows: &BTreeMap<String, LlmModelRecord>, r: &LlmModelRecord, name: &str) -> bool {
    rows.values().any(|o| {
        o.id != r.id && o.user_id == r.user_id && o.provider == r.provider && o.name == name
    })
}

#[async_trait]
impl LlmModelRepository for InMemoryLlmModelRepository {
    async fn insert(
        &self,
        user_id: &str,
        provider: &str,
        name: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<LlmModelRecord, LlmModelRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if st
            .rows
            .values()
            .any(|o| o.user_id == user_id && o.provider == provider && o.name == name)
        {
            return Err(LlmModelRepoError::Duplicate);
        }
        st.seq += 1;
        let rec = LlmModelRecord {
            id: format!("llm-{}", st.seq),
            user_id: user_id.to_string(),
            provider: provider.to_string(),
            name: name.to_string(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            enabled: true,
            created_at_ms: now_ms(),
        };
        st.rows.insert(rec.id.clone(), rec.clone());
        Ok(rec)
    }

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<LlmModelRecord>, LlmModelRepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.rows.values().filter(|r| r.user_id == user_id).cloned().collect())
    }

    async fn update(
        &self,
        user_id: &str,
        id: &str,
        patch: LlmModelPatch,
    ) -> Result<Option<LlmModelRecord>, LlmModelRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(current) = st.rows.get(id).filter(|r| r.user_id == user_id).cloned() else {
            return Ok(None);
        };
        if let Some(name) = &patch.name {
            if dup(&st.rows, &current, name) {
                return Err(LlmModelRepoError::Duplicate);
            }
        }
        let mut updated = current;
        if let Some(name) = patch.name {
            updated.name = name;
        }
        if let Some(base_url) = patch.base_url {
            updated.base_url = base_url;
        }
        if let Some(api_key) = patch.api_key {
            updated.api_key = api_key;
        }
        if let Some(enabled) = patch.enabled {
            updated.enabled = enabled;
        }
        st.rows.insert(id.to_string(), updated.clone());
        Ok(Some(updated))
    }

    async fn delete(&self, user_id: &str, id: &str) -> Result<bool, LlmModelRepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match st.rows.get(id) {
            Some(r) if r.user_id == user_id => {
                st.rows.remove(id);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
