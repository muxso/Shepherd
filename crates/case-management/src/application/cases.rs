use std::collections::BTreeMap;
use std::sync::Arc;

use thiserror::Error;

use crate::domain::{CaseError, FunctionalCase, NewFunctionalCase};
use crate::ports::{CaseRepository, RepoError};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateCaseError {
    #[error(transparent)]
    Validation(#[from] CaseError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateCaseUseCase {
    repo: Arc<dyn CaseRepository>,
}

impl CreateCaseUseCase {
    pub fn new(repo: Arc<dyn CaseRepository>) -> Self {
        Self { repo }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        project_id: &str,
        name: &str,
        module: &str,
        priority: &str,
        status: &str,
        custom_fields: BTreeMap<String, String>,
        steps: Vec<crate::domain::CaseStep>,
        created_by: Option<&str>,
    ) -> Result<FunctionalCase, CreateCaseError> {
        self.execute_with_tags(
            project_id,
            name,
            module,
            priority,
            status,
            Vec::new(),
            custom_fields,
            steps,
            created_by,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_tags(
        &self,
        project_id: &str,
        name: &str,
        module: &str,
        priority: &str,
        status: &str,
        tags: Vec<String>,
        custom_fields: BTreeMap<String, String>,
        steps: Vec<crate::domain::CaseStep>,
        created_by: Option<&str>,
    ) -> Result<FunctionalCase, CreateCaseError> {
        let new = NewFunctionalCase::new(
            project_id,
            name,
            module,
            priority,
            status,
            custom_fields,
            steps,
        )?
        .with_created_by(created_by)
        .with_tags(tags);
        let created = self.repo.insert(&new).await?;
        let entry = ("create".to_string(), String::new(), created.name.clone());
        self.repo.record_changes(&created.id, &[entry], created_by.unwrap_or_default()).await?;
        Ok(created)
    }
}

#[derive(Clone)]
pub struct UpdateCaseUseCase {
    repo: Arc<dyn CaseRepository>,
}

impl UpdateCaseUseCase {
    pub fn new(repo: Arc<dyn CaseRepository>) -> Self {
        Self { repo }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        id: &str,
        project_id: &str,
        name: &str,
        module: &str,
        priority: &str,
        status: &str,
        custom_fields: BTreeMap<String, String>,
        steps: Vec<crate::domain::CaseStep>,
    ) -> Result<Option<FunctionalCase>, CreateCaseError> {
        self.execute_with_tags(
            id,
            project_id,
            name,
            module,
            priority,
            status,
            Vec::new(),
            custom_fields,
            steps,
            "",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_tags(
        &self,
        id: &str,
        project_id: &str,
        name: &str,
        module: &str,
        priority: &str,
        status: &str,
        tags: Vec<String>,
        custom_fields: BTreeMap<String, String>,
        steps: Vec<crate::domain::CaseStep>,
        actor: &str,
    ) -> Result<Option<FunctionalCase>, CreateCaseError> {
        let new = NewFunctionalCase::new(
            project_id,
            name,
            module,
            priority,
            status,
            custom_fields,
            steps,
        )?
        .with_tags(tags);
        let old = self.repo.get(id).await?;
        let updated = self.repo.update(id, &new).await?;
        if let (Some(old), Some(new_case)) = (old, updated.as_ref()) {
            let changes = diff_case(&old, new_case);
            if !changes.is_empty() {
                self.repo.record_changes(id, &changes, actor).await?;
            }
        }
        Ok(updated)
    }
}

/// Field-level diff of two case snapshots as (field, old, new) audit entries.
fn diff_case(old: &FunctionalCase, new: &FunctionalCase) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut push = |field: &str, o: String, n: String| {
        if o != n {
            out.push((field.to_string(), o, n));
        }
    };
    push("name", old.name.clone(), new.name.clone());
    push("module", old.module.clone(), new.module.clone());
    push("priority", old.priority.clone(), new.priority.clone());
    push("status", old.status.clone(), new.status.clone());
    push("tags", old.tags.join(", "), new.tags.join(", "));
    if old.steps != new.steps {
        let fmt = |steps: &[crate::domain::CaseStep]| {
            steps
                .iter()
                .map(|s| format!("{} => {}", s.step, s.expected))
                .collect::<Vec<_>>()
                .join("\n")
        };
        push("steps", fmt(&old.steps), fmt(&new.steps));
    }
    let keys: std::collections::BTreeSet<&String> =
        old.custom_fields.keys().chain(new.custom_fields.keys()).collect();
    for k in keys {
        let o = old.custom_fields.get(k).cloned().unwrap_or_default();
        let n = new.custom_fields.get(k).cloned().unwrap_or_default();
        push(&format!("field.{k}"), o, n);
    }
    out
}

#[derive(Clone)]
pub struct DeleteCaseUseCase {
    repo: Arc<dyn CaseRepository>,
}

impl DeleteCaseUseCase {
    pub fn new(repo: Arc<dyn CaseRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, id: &str) -> Result<bool, RepoError> {
        self.repo.delete(id).await
    }
}

#[derive(Clone)]
pub struct ImportCasesUseCase {
    repo: Arc<dyn CaseRepository>,
}

impl ImportCasesUseCase {
    pub fn new(repo: Arc<dyn CaseRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        rows: &[Vec<String>],
        created_by: Option<&str>,
    ) -> Result<usize, RepoError> {
        let news = cases_from_rows(project_id, rows);
        let mut n = 0;
        for new in news {
            self.repo.insert(&new.with_created_by(created_by)).await?;
            n += 1;
        }
        Ok(n)
    }
}

#[derive(Clone)]
pub struct ListCasesUseCase {
    repo: Arc<dyn CaseRepository>,
}

impl ListCasesUseCase {
    pub fn new(repo: Arc<dyn CaseRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, project_id: &str) -> Result<Vec<FunctionalCase>, RepoError> {
        self.repo.list_by_project(project_id).await
    }
}

pub fn cases_from_rows(project_id: &str, rows: &[Vec<String>]) -> Vec<NewFunctionalCase> {
    let Some(header) = rows.first() else { return Vec::new() };
    let (mut name_i, mut module_i, mut prio_i, mut status_i) = (None, None, None, None);
    let mut custom_cols: Vec<(usize, String)> = Vec::new();
    for (i, h) in header.iter().enumerate() {
        match h.trim() {
            "ID" => {}
            "名称" => name_i = Some(i),
            "模块" => module_i = Some(i),
            "优先级" => prio_i = Some(i),
            "状态" => status_i = Some(i),
            other if !other.is_empty() => custom_cols.push((i, other.to_string())),
            _ => {}
        }
    }
    rows.iter()
        .skip(1)
        .filter_map(|row| {
            let custom: BTreeMap<String, String> = custom_cols
                .iter()
                .filter_map(|(i, k)| {
                    row.get(*i).filter(|v| !v.trim().is_empty()).map(|v| (k.clone(), v.clone()))
                })
                .collect();
            NewFunctionalCase::new(
                project_id,
                cell(row, name_i),
                cell(row, module_i),
                cell(row, prio_i),
                cell(row, status_i),
                custom,
                Vec::new(),
            )
            .ok()
        })
        .collect()
}

fn cell(row: &[String], idx: Option<usize>) -> &str {
    idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("")
}

pub fn export_rows(cases: &[FunctionalCase]) -> Vec<Vec<String>> {
    let mut field_names: Vec<String> =
        cases.iter().flat_map(|c| c.custom_fields.keys().cloned()).collect();
    field_names.sort();
    field_names.dedup();

    let mut header = vec![
        "ID".to_string(),
        "名称".to_string(),
        "模块".to_string(),
        "优先级".to_string(),
        "状态".to_string(),
    ];
    header.extend(field_names.iter().cloned());

    let mut rows = vec![header];
    for c in cases {
        let mut row = vec![
            c.id.clone(),
            c.name.clone(),
            c.module.clone(),
            c.priority.clone(),
            c.status.clone(),
        ];
        for f in &field_names {
            row.push(c.custom_fields.get(f).cloned().unwrap_or_default());
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryCaseRepository;

    fn case(id: &str, name: &str, fields: &[(&str, &str)]) -> FunctionalCase {
        FunctionalCase {
            id: id.into(),
            project_id: "p1".into(),
            num: 0,
            name: name.into(),
            module: "login".into(),
            priority: "P2".into(),
            status: "PREPARED".into(),
            tags: Vec::new(),
            custom_fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            steps: Vec::new(),
            created_by: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[tokio::test]
    async fn create_then_list() {
        let repo = Arc::new(InMemoryCaseRepository::new());
        let create = CreateCaseUseCase::new(repo.clone());
        let list = ListCasesUseCase::new(repo);
        create
            .execute(
                "p1",
                "login success",
                "login",
                "P0",
                "",
                BTreeMap::new(),
                Vec::new(),
                Some("alice"),
            )
            .await
            .expect("created");
        let all = list.execute("p1").await.expect("listed");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "login success");
        assert_eq!(all[0].status, "PREPARED");
        assert_eq!(all[0].created_by.as_deref(), Some("alice"));
    }

    #[test]
    fn cases_from_rows_is_inverse_of_export() {
        let cases = vec![
            case("c1", "case 1", &[("owner", "alice")]),
            case("c2", "case 2", &[("sprint", "S1")]),
        ];
        let rows = export_rows(&cases);
        let news = cases_from_rows("p1", &rows);
        assert_eq!(news.len(), 2);
        assert_eq!(news[0].name, "case 1");
        assert_eq!(news[0].module, "login");
        assert_eq!(news[0].custom_fields.get("owner").map(String::as_str), Some("alice"));
        assert_eq!(news[1].custom_fields.get("sprint").map(String::as_str), Some("S1"));
    }

    #[test]
    fn cases_from_rows_skips_empty_name() {
        let rows = vec![
            vec!["名称".into(), "模块".into()],
            vec!["".into(), "login".into()],
            vec!["valid case".into(), "login".into()],
        ];
        let news = cases_from_rows("p1", &rows);
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].name, "valid case");
    }

    #[tokio::test]
    async fn import_inserts_cases() {
        let repo = Arc::new(InMemoryCaseRepository::new());
        let import = ImportCasesUseCase::new(repo.clone());
        let list = ListCasesUseCase::new(repo);
        let rows = vec![
            vec!["名称".into(), "优先级".into(), "owner".into()],
            vec!["login success".into(), "P0".into(), "alice".into()],
            vec!["wrong password".into(), "P1".into(), "bob".into()],
        ];
        assert_eq!(import.execute("p1", &rows, Some("importer")).await.expect("import"), 2);
        let all = list.execute("p1").await.expect("list");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].priority, "P0");
        assert_eq!(all[0].custom_fields["owner"], "alice");
        assert_eq!(all[0].created_by.as_deref(), Some("importer"));
    }

    #[test]
    fn export_rows_unions_custom_fields() {
        let cases = vec![
            case("c1", "case 1", &[("owner", "alice")]),
            case("c2", "case 2", &[("sprint", "S1")]),
        ];
        let rows = export_rows(&cases);
        assert_eq!(rows[0], vec!["ID", "名称", "模块", "优先级", "状态", "owner", "sprint"]);
        assert_eq!(rows[1], vec!["c1", "case 1", "login", "P2", "PREPARED", "alice", ""]);
        assert_eq!(rows[2], vec!["c2", "case 2", "login", "P2", "PREPARED", "", "S1"]);
    }
}
