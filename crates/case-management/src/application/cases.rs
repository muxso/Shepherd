//! 用例:创建 / 列出功能用例 + 导出行(纯函数)。

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
    ) -> Result<FunctionalCase, CreateCaseError> {
        let new = NewFunctionalCase::new(project_id, name, module, priority, status, custom_fields)?;
        Ok(self.repo.insert(&new).await?)
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

/// 导出表格行(纯函数):表头 + 每用例一行。自定义字段列由所有用例字段名并集决定,
/// 列顺序确定(固定列在前、自定义字段名按字典序),便于 Excel/CSV 编码。
pub fn export_rows(cases: &[FunctionalCase]) -> Vec<Vec<String>> {
    // 收集所有出现过的自定义字段名(并集、去重、排序)。
    let mut field_names: Vec<String> = cases
        .iter()
        .flat_map(|c| c.custom_fields.keys().cloned())
        .collect();
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
            name: name.into(),
            module: "登录".into(),
            priority: "P2".into(),
            status: "PREPARED".into(),
            custom_fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[tokio::test]
    async fn create_then_list() {
        let repo = Arc::new(InMemoryCaseRepository::new());
        let create = CreateCaseUseCase::new(repo.clone());
        let list = ListCasesUseCase::new(repo);
        create
            .execute("p1", "登录成功", "登录", "P0", "", BTreeMap::new())
            .await
            .expect("created");
        let all = list.execute("p1").await.expect("listed");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "登录成功");
        assert_eq!(all[0].status, "PREPARED"); // 缺省
    }

    #[test]
    fn export_rows_unions_custom_fields() {
        let cases = vec![
            case("c1", "用例1", &[("owner", "alice")]),
            case("c2", "用例2", &[("sprint", "S1")]),
        ];
        let rows = export_rows(&cases);
        // 表头:5 固定列 + 自定义字段并集(owner, sprint 按字典序)
        assert_eq!(rows[0], vec!["ID", "名称", "模块", "优先级", "状态", "owner", "sprint"]);
        // c1 有 owner 无 sprint
        assert_eq!(rows[1], vec!["c1", "用例1", "登录", "P2", "PREPARED", "alice", ""]);
        // c2 无 owner 有 sprint
        assert_eq!(rows[2], vec!["c2", "用例2", "登录", "P2", "PREPARED", "", "S1"]);
    }
}
