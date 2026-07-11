//! 拆分后自动起草功能测试用例(第四个 AI 触点,前三个见 llm.rs:拆分/执行/验证)。
//! 输入 = 需求 + 刚拆出的任务;输出 = 带步骤的用例草稿,由 breakdown 路由落库并关联验收标准。
//! 未配置 LLM 或起草失败时,回落到按任务的确定性模板(每任务一条,步骤来自任务验收标准)。

use async_trait::async_trait;

use task::domain::Task;
use task::ports::RequirementSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftedStep {
    pub step: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftedCase {
    pub name: String,
    pub steps: Vec<DraftedStep>,
    /// 覆盖的需求级验收标准下标(0 起);落库时逐条建立覆盖关联。
    pub criterion_indexes: Vec<i32>,
}

#[async_trait]
pub trait CaseDrafter: Send + Sync {
    async fn draft(
        &self,
        spec: &RequirementSpec,
        tasks: &[Task],
    ) -> Result<Vec<DraftedCase>, String>;
}

/// 确定性模板:每任务一条用例;步骤 = 该任务的验收标准逐条「验证 → 预期」;
/// 与需求级标准文本相同的条目回链覆盖关系。
pub fn template_cases(spec: &RequirementSpec, tasks: &[Task]) -> Vec<DraftedCase> {
    tasks
        .iter()
        .map(|t| {
            let steps: Vec<DraftedStep> = if t.acceptance_criteria.is_empty() {
                vec![DraftedStep {
                    step: format!("执行任务「{}」对应的功能路径", t.title),
                    expected: "功能按任务描述正常工作".to_string(),
                }]
            } else {
                t.acceptance_criteria
                    .iter()
                    .map(|c| DraftedStep {
                        step: format!("针对「{}」验证:{}", t.title, c),
                        expected: c.clone(),
                    })
                    .collect()
            };
            let criterion_indexes = t
                .acceptance_criteria
                .iter()
                .filter_map(|c| spec.acceptance_criteria.iter().position(|s| s == c))
                .map(|i| i as i32)
                .collect();
            DraftedCase { name: format!("验证:{}", t.title), steps, criterion_indexes }
        })
        .collect()
}

/// LLM 起草结果解析(独立成函数便于单测):越界下标丢弃、空名/空步骤跳过、总量封顶 3×任务数。
pub fn parse_drafted(
    text: &str,
    task_count: usize,
    criteria_count: usize,
) -> Result<Vec<DraftedCase>, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct StepDto {
        #[serde(default)]
        step: String,
        #[serde(default)]
        expected: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CaseDto {
        #[serde(default)]
        name: String,
        #[serde(default)]
        criterion_indexes: Vec<i64>,
        #[serde(default)]
        steps: Vec<StepDto>,
    }
    let dtos: Vec<CaseDto> =
        serde_json::from_str(text).map_err(|e| format!("用例草稿解析失败: {e}"))?;
    let cap = task_count.max(1) * 3;
    let out: Vec<DraftedCase> = dtos
        .into_iter()
        .filter(|d| !d.name.trim().is_empty())
        .map(|d| DraftedCase {
            name: d.name.trim().to_string(),
            steps: d
                .steps
                .into_iter()
                .filter(|s| !s.step.trim().is_empty())
                .map(|s| DraftedStep {
                    step: s.step.trim().to_string(),
                    expected: s.expected.trim().to_string(),
                })
                .collect(),
            criterion_indexes: d
                .criterion_indexes
                .into_iter()
                .filter(|i| *i >= 0 && (*i as usize) < criteria_count)
                .map(|i| i as i32)
                .collect(),
        })
        .filter(|c| !c.steps.is_empty())
        .take(cap)
        .collect();
    if out.is_empty() {
        return Err("用例草稿为空".to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use task::domain::TaskStatus;

    fn spec(criteria: &[&str]) -> RequirementSpec {
        RequirementSpec {
            requirement_id: "r1".into(),
            requirement_version: 1,
            title: "登录".into(),
            description: "手机号登录".into(),
            acceptance_criteria: criteria.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn task(title: &str, criteria: &[&str]) -> Task {
        Task {
            id: "t1".into(),
            title: title.into(),
            description: String::new(),
            acceptance_criteria: criteria.iter().map(|s| s.to_string()).collect(),
            dependencies: vec![],
            status: TaskStatus::Pending,
            points: 0,
            assignee: String::new(),
            assignee_kind: String::new(),
        }
    }

    #[test]
    fn template_builds_one_case_per_task_with_criterion_links() {
        let s = spec(&["登录成功", "错误密码拒绝"]);
        let ts = vec![task("实现登录接口", &["登录成功"]), task("错误处理", &["错误密码拒绝"])];
        let cases = template_cases(&s, &ts);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].name, "验证:实现登录接口");
        assert_eq!(cases[0].steps.len(), 1);
        assert_eq!(cases[0].criterion_indexes, vec![0]);
        assert_eq!(cases[1].criterion_indexes, vec![1]);
    }

    #[test]
    fn template_task_without_criteria_gets_default_step() {
        let s = spec(&[]);
        let cases = template_cases(&s, &[task("搭脚手架", &[])]);
        assert_eq!(cases[0].steps.len(), 1);
        assert!(cases[0].criterion_indexes.is_empty());
    }

    #[test]
    fn parse_drops_invalid_and_caps() {
        let text = r#"[
          {"name":"用例A","criterionIndexes":[0, 9],"steps":[{"step":"打开登录页","expected":"显示表单"}]},
          {"name":"  ","steps":[{"step":"x","expected":"y"}]},
          {"name":"无步骤","steps":[]}
        ]"#;
        let cases = parse_drafted(text, 1, 2).expect("parse");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].criterion_indexes, vec![0]); // 9 越界被丢
    }

    #[test]
    fn parse_rejects_empty_result() {
        assert!(parse_drafted("[]", 2, 2).is_err());
        assert!(parse_drafted("not json", 2, 2).is_err());
    }
}
