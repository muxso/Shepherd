use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::{
    parse_import, ApiDefinition, ApiDefinitionError, ApiProtocol, ImportFormat, NewApiCase,
    NewApiDefinition, NewApiModule,
};
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error(transparent)]
    Parse(#[from] ApiDefinitionError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub module_id: Option<String>,
    pub group_by_tag: bool,
    pub overwrite: bool,
    pub sync_module: bool,
}

#[derive(Debug)]
pub struct ImportOutcome {
    pub created: Vec<ApiDefinition>,
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Clone)]
pub struct ImportApiDefinitionsUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl ImportApiDefinitionsUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        format: ImportFormat,
        doc: &serde_json::Value,
        opts: ImportOptions,
    ) -> Result<ImportOutcome, ImportError> {
        let module_id = opts.module_id.as_deref();
        let group_by_tag = opts.group_by_tag;
        let overwrite = opts.overwrite;
        let sync_module = opts.sync_module;
        let apis = parse_import(format, doc)?;

        let existing = self.repo.list_definitions(project_id).await?;
        let index: HashMap<(String, String), String> = existing
            .into_iter()
            .map(|d| ((d.method.to_uppercase(), d.path.clone()), d.id))
            .collect();

        let mut module_by_tag: HashMap<String, String> = HashMap::new();
        if group_by_tag {
            let parent = module_id.map(str::to_string);
            for m in self.repo.list_modules(project_id).await? {
                if m.parent_id == parent {
                    module_by_tag.insert(m.name.clone(), m.id);
                }
            }
        }

        let mut created = Vec::new();
        let mut updated = 0usize;
        let mut skipped = 0usize;
        for api in apis {
            let new_def = match NewApiDefinition::new(
                project_id,
                &api.name,
                ApiProtocol::Http,
                &api.method,
                &api.path,
            ) {
                Ok(d) => d.with_spec(&api.spec.to_string()),
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };

            let api_module = api.module.clone();

            if let Some(id) = index.get(&(new_def.method.to_uppercase(), new_def.path.clone())) {
                if !overwrite {
                    skipped += 1;
                    continue;
                }
                if self.repo.update_definition_spec(id, &new_def.spec).await.is_ok() {
                    let _ = self
                        .repo
                        .record_definition_change(
                            id,
                            "UPDATE_SPEC",
                            "OpenAPI import overwrote spec",
                            "",
                        )
                        .await;
                    if sync_module {
                        let mid = self
                            .resolve_module(
                                project_id,
                                module_id,
                                group_by_tag,
                                api_module.as_deref(),
                                &mut module_by_tag,
                            )
                            .await;
                        if let Some(mid) = mid.as_deref() {
                            let _ = self.repo.set_definition_module(id, Some(mid)).await;
                        }
                    }
                    updated += 1;
                } else {
                    skipped += 1;
                }
                continue;
            }

            let mut def = self.repo.insert_definition(&new_def).await?;
            let mid = self
                .resolve_module(
                    project_id,
                    module_id,
                    group_by_tag,
                    api_module.as_deref(),
                    &mut module_by_tag,
                )
                .await;
            if let Some(mid) = mid.as_deref() {
                if self.repo.set_definition_module(&def.id, Some(mid)).await.is_ok() {
                    def.module_id = Some(mid.to_string());
                }
            }
            let case_name = format!("{} default case", api.name);
            if let Ok(case) = NewApiCase::new(
                &def.id,
                project_id,
                &case_name,
                &api.method,
                &api.path,
                api.case_body.clone(),
                api.case_assertions.clone(),
            ) {
                let _ = self.repo.insert_case(&case).await;
            }
            created.push(def);
        }
        Ok(ImportOutcome { created, updated, skipped })
    }

    async fn resolve_module(
        &self,
        project_id: &str,
        parent: Option<&str>,
        group_by_tag: bool,
        tag: Option<&str>,
        cache: &mut HashMap<String, String>,
    ) -> Option<String> {
        let tag = match (group_by_tag, tag) {
            (true, Some(t)) if !t.trim().is_empty() => t.trim(),
            _ => return parent.map(str::to_string),
        };
        if let Some(id) = cache.get(tag) {
            return Some(id.clone());
        }
        match NewApiModule::new(project_id, parent, tag) {
            Ok(nm) => match self.repo.insert_module(&nm).await {
                Ok(m) => {
                    cache.insert(tag.to_string(), m.id.clone());
                    Some(m.id)
                }
                Err(_) => parent.map(str::to_string),
            },
            Err(_) => parent.map(str::to_string),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::ports::ApiDefinitionRepository;
    use serde_json::json;

    #[tokio::test]
    async fn imports_openapi_creates_one_definition_per_operation() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = ImportApiDefinitionsUseCase::new(repo.clone());
        let doc = json!({
            "openapi": "3.0.0",
            "paths": {
                "/login": { "post": { "summary": "login" } },
                "/users": { "get": { "operationId": "listUsers" } }
            }
        });
        let out = uc
            .execute(
                "p1",
                ImportFormat::Openapi,
                &doc,
                ImportOptions { overwrite: true, ..Default::default() },
            )
            .await
            .expect("imported");
        assert_eq!(out.created.len(), 2);
        assert_eq!(out.skipped, 0);
        assert_eq!(repo.list_definitions("p1").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn group_by_tag_creates_module_per_tag_and_assigns() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = ImportApiDefinitionsUseCase::new(repo.clone());
        let doc = json!({
            "openapi": "3.0.0",
            "paths": {
                "/login": { "post": { "summary": "login", "tags": ["auth"] } },
                "/logout": { "post": { "summary": "logout", "tags": ["auth"] } },
                "/users": { "get": { "summary": "list users", "tags": ["user"] } },
                "/ping": { "get": { "summary": "ping" } }
            }
        });
        let out = uc
            .execute(
                "p1",
                ImportFormat::Openapi,
                &doc,
                ImportOptions { group_by_tag: true, overwrite: true, ..Default::default() },
            )
            .await
            .expect("imported");
        assert_eq!(out.created.len(), 4);
        let mods = repo.list_modules("p1").await.unwrap();
        assert_eq!(mods.len(), 2);
        let by_name: HashMap<_, _> = mods.iter().map(|m| (m.name.clone(), m.id.clone())).collect();
        assert!(by_name.contains_key("auth") && by_name.contains_key("user"));
        let defs = repo.list_definitions("p1").await.unwrap();
        let mid_of = |path: &str| defs.iter().find(|d| d.path == path).unwrap().module_id.clone();
        assert_eq!(mid_of("/login"), Some(by_name["auth"].clone()));
        assert_eq!(mid_of("/logout"), Some(by_name["auth"].clone()));
        assert_eq!(mid_of("/users"), Some(by_name["user"].clone()));
        assert_eq!(mid_of("/ping"), None);
    }

    #[tokio::test]
    async fn unparseable_doc_errors() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let uc = ImportApiDefinitionsUseCase::new(repo);
        let err = uc
            .execute(
                "p1",
                ImportFormat::Openapi,
                &json!({"foo": 1}),
                ImportOptions { overwrite: true, ..Default::default() },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ImportError::Parse(ApiDefinitionError::BadImport(_))));
    }
}
