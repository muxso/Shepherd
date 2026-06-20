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

/// 批量导入:解析行 → 逐条创建。返回成功导入条数。
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
    ) -> Result<usize, RepoError> {
        let news = cases_from_rows(project_id, rows);
        let mut n = 0;
        for new in &news {
            self.repo.insert(new).await?;
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

/// 从表格行(纯函数)解析出待创建用例:`export_rows` 的逆。
/// 表头按列名识别固定列(ID 列忽略;名称/模块/优先级/状态),其余列名 = 自定义字段。
/// 名称为空的行跳过(校验失败即丢弃,容忍脏数据)。
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
            )
            .ok()
        })
        .collect()
}

/// 取行中某列(下标缺失/越界回落空串)。
fn cell(row: &[String], idx: Option<usize>) -> &str {
    idx.and_then(|i| row.get(i)).map(|s| s.as_str()).unwrap_or("")
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
    fn cases_from_rows_is_inverse_of_export() {
        let cases = vec![
            case("c1", "用例1", &[("owner", "alice")]),
            case("c2", "用例2", &[("sprint", "S1")]),
        ];
        let rows = export_rows(&cases);
        let news = cases_from_rows("p1", &rows);
        assert_eq!(news.len(), 2);
        assert_eq!(news[0].name, "用例1");
        assert_eq!(news[0].module, "登录");
        assert_eq!(news[0].custom_fields.get("owner").map(String::as_str), Some("alice"));
        assert_eq!(news[1].custom_fields.get("sprint").map(String::as_str), Some("S1"));
    }

    #[test]
    fn cases_from_rows_skips_empty_name() {
        let rows = vec![
            vec!["名称".into(), "模块".into()],
            vec!["".into(), "登录".into()], // 空名 → 跳过
            vec!["有效用例".into(), "登录".into()],
        ];
        let news = cases_from_rows("p1", &rows);
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].name, "有效用例");
    }

    #[tokio::test]
    async fn import_inserts_cases() {
        let repo = Arc::new(InMemoryCaseRepository::new());
        let import = ImportCasesUseCase::new(repo.clone());
        let list = ListCasesUseCase::new(repo);
        let rows = vec![
            vec!["名称".into(), "优先级".into(), "owner".into()],
            vec!["登录成功".into(), "P0".into(), "alice".into()],
            vec!["密码错误".into(), "P1".into(), "bob".into()],
        ];
        assert_eq!(import.execute("p1", &rows).await.expect("import"), 2);
        let all = list.execute("p1").await.expect("list");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].priority, "P0");
        assert_eq!(all[0].custom_fields["owner"], "alice");
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
