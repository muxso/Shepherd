//! 用例:导入接口定义(OpenAPI 3.x / Swagger 2.0)。
//!
//! 解析文档为一批接口(纯函数 [`parse_openapi`]),逐条建为接口定义。整体「尽力而为」:
//! 单条建失败不阻断其余,返回成功建出的定义 + 跳过的条数,便于前端反馈导入结果。

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
    /// 文档无法解析为 OpenAPI/Swagger。
    #[error(transparent)]
    Parse(#[from] ApiDefinitionError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// 导入结果:新建的定义 + 覆盖更新的条数 + 跳过条数(校验失败但不阻断整体的)。
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

    /// `doc` 为解析后的 JSON 文档(JMeter 为含原始 .jmx 文本的 JSON 字符串)。先按 `format` 解析
    /// (失败整体报错),再逐条建定义。
    /// `module_id`:新建定义归入的模块(None=未归类);`group_by_tag` 开启时它作为按 tag 自建模块的**父级**。
    /// `group_by_tag`:按来源的标签/分组(OpenAPI tag、Postman 文件夹、HAR host、MS 模块)自动建/复用子模块并归类(无标签回落 `module_id`)。
    /// `overwrite`:同 方法+路径 已存在时 true=覆盖其 spec(默认),false=不覆盖(跳过,计入 skipped)。
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        project_id: &str,
        format: ImportFormat,
        doc: &serde_json::Value,
        module_id: Option<&str>,
        group_by_tag: bool,
        overwrite: bool,
        sync_module: bool,
    ) -> Result<ImportOutcome, ImportError> {
        let apis = parse_import(format, doc)?;

        // 幂等导入:按「方法 + 路径」识别项目内已存在的定义。命中则覆盖其 spec(保留用户已编辑的用例),
        // 未命中则新建定义 + 默认用例。避免重复导入同一份 OpenAPI 堆出重复接口。
        let existing = self.repo.list_definitions(project_id).await?;
        let index: HashMap<(String, String), String> = existing
            .into_iter()
            .map(|d| ((d.method.to_uppercase(), d.path.clone()), d.id))
            .collect();

        // 按 tag 自动归类:预载选定父级下的现有子模块(name→id),供「建或复用」,幂等不堆重复模块。
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
            let new_def =
                match NewApiDefinition::new(project_id, &api.name, ApiProtocol::Http, &api.method, &api.path)
                {
                    Ok(d) => d.with_spec(&api.spec.to_string()),
                    Err(_) => {
                        skipped += 1; // 单条校验失败:跳过,不阻断整体
                        continue;
                    }
                };

            // 该接口的目标模块:group_by_tag → 按首个 tag 建/复用子模块(父级=module_id;无 tag 回落 module_id);
            // 否则用 module_id。延迟到需要落库时再解析,避免覆盖且不同步时建出空模块。
            let api_module = api.module.clone();

            // 已存在同 方法+路径:覆盖模式刷新 spec(不动既有用例);不覆盖模式跳过。
            if let Some(id) = index.get(&(new_def.method.to_uppercase(), new_def.path.clone())) {
                if !overwrite {
                    skipped += 1;
                    continue;
                }
                if self.repo.update_definition_spec(id, &new_def.spec).await.is_ok() {
                    let _ = self
                        .repo
                        .record_definition_change(id, "UPDATE_SPEC", "OpenAPI 导入覆盖规格", "")
                        .await;
                    // 同步更新接口所在目录:把已存在接口移到目标模块(按 tag 或选定模块)。
                    if sync_module {
                        let mid = self
                            .resolve_module(project_id, module_id, group_by_tag, api_module.as_deref(), &mut module_by_tag)
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

            // 新接口:建定义 + 默认用例。URL 取相对路径(执行时由环境 baseUrl 拼接),
            // 断言含状态码 + 基础业务断言(由 OpenAPI 成功响应派生)。建用例尽力而为,失败不阻断导入。
            let mut def = self.repo.insert_definition(&new_def).await?;
            // 归入目标模块(尽力而为);回写返回对象的 module_id,使响应与落库一致。
            let mid = self
                .resolve_module(project_id, module_id, group_by_tag, api_module.as_deref(), &mut module_by_tag)
                .await;
            if let Some(mid) = mid.as_deref() {
                if self.repo.set_definition_module(&def.id, Some(mid)).await.is_ok() {
                    def.module_id = Some(mid.to_string());
                }
            }
            let case_name = format!("{} 默认用例", api.name);
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

    /// 解析某接口的目标模块 id。`group_by_tag` 关闭或无 tag → 回落 `parent`(选定模块/未归类)。
    /// 有 tag → 缓存命中复用,否则在 `parent` 下建子模块并缓存(建失败回落 `parent`)。
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
                "/login": { "post": { "summary": "登录" } },
                "/users": { "get": { "operationId": "listUsers" } }
            }
        });
        let out =
            uc.execute("p1", ImportFormat::Openapi, &doc, None, false, true, false).await.expect("imported");
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
                "/login": { "post": { "summary": "登录", "tags": ["auth"] } },
                "/logout": { "post": { "summary": "登出", "tags": ["auth"] } },
                "/users": { "get": { "summary": "列用户", "tags": ["user"] } },
                "/ping": { "get": { "summary": "ping" } }
            }
        });
        let out =
            uc.execute("p1", ImportFormat::Openapi, &doc, None, true, true, false).await.expect("imported");
        assert_eq!(out.created.len(), 4);
        // 两个 tag → 两个模块(auth 复用,不重复建)
        let mods = repo.list_modules("p1").await.unwrap();
        assert_eq!(mods.len(), 2);
        let by_name: HashMap<_, _> = mods.iter().map(|m| (m.name.clone(), m.id.clone())).collect();
        assert!(by_name.contains_key("auth") && by_name.contains_key("user"));
        // 定义归入对应模块;无 tag 的 ping 仍未归类
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
            .execute("p1", ImportFormat::Openapi, &json!({"foo": 1}), None, false, true, false)
            .await
            .unwrap_err();
        assert!(matches!(err, ImportError::Parse(ApiDefinitionError::BadImport(_))));
    }
}
