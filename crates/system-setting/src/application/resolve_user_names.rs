//! Name resolution only uses `names_direct`, never the validation path — that would 500
//! for OIDC users. On error it degrades to an empty map.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ports::UserDirectory;

#[derive(Clone)]
pub struct ResolveUserNamesUseCase {
    directory: Arc<dyn UserDirectory>,
}

impl ResolveUserNamesUseCase {
    pub fn new(directory: Arc<dyn UserDirectory>) -> Self {
        Self { directory }
    }

    pub async fn execute(&self, ids: &[String]) -> BTreeMap<String, String> {
        if ids.is_empty() {
            return BTreeMap::new();
        }
        self.directory.names_direct(ids).await.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::SpyDirectory;
    use crate::domain::UserSource;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn resolves_oidc_user_names() {
        let dir = SpyDirectory::new().with_user("u1", "Alice", UserSource::Oidc).with_user(
            "u2",
            "Bob",
            UserSource::Local,
        );
        let uc = ResolveUserNamesUseCase::new(Arc::new(dir));
        let names = uc.execute(&ids(&["u1", "u2"])).await;
        assert_eq!(names.get("u1"), Some(&"Alice".to_string()));
        assert_eq!(names.get("u2"), Some(&"Bob".to_string()));
    }

    #[tokio::test]
    async fn never_touches_the_cft_validated_path() {
        let dir = SpyDirectory::new().with_user("u1", "Alice", UserSource::Oidc);
        let spy = dir.clone();
        let uc = ResolveUserNamesUseCase::new(Arc::new(dir));
        uc.execute(&ids(&["u1"])).await;
        assert_eq!(spy.validated_calls(), 0);
        assert_eq!(spy.direct_calls(), 1);
    }

    #[tokio::test]
    async fn degrades_to_empty_when_direct_store_errors() {
        let dir = SpyDirectory::new().with_user("u1", "Alice", UserSource::Oidc).failing_direct();
        let uc = ResolveUserNamesUseCase::new(Arc::new(dir));
        assert!(uc.execute(&ids(&["u1"])).await.is_empty());
    }

    #[tokio::test]
    async fn empty_input_short_circuits() {
        let dir = SpyDirectory::new();
        let spy = dir.clone();
        let uc = ResolveUserNamesUseCase::new(Arc::new(dir));
        assert!(uc.execute(&[]).await.is_empty());
        assert_eq!(spy.direct_calls(), 0);
        assert_eq!(spy.validated_calls(), 0);
    }
}
