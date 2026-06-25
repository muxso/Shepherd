//! 认领到的工作规格(对应 server `/agent/work/claim` 的 WorkSpecDto,camelCase)。

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSpec {
    pub attempt_id: String,
    // 保留完整 claim 字段:runtime 暂不用,留作对账 / 未来用。
    #[serde(default)]
    #[allow(dead_code)]
    pub decomposition_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub task_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub executor: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub instructions: Option<String>,
}

impl WorkSpec {
    /// design 模式(产设计稿→/proposal/{id}/design)由 drafter 经 context 标记。
    pub fn is_design(&self) -> bool {
        self.context.as_deref() == Some("design")
    }

    /// 组装交给 CLI 的 prompt(对齐 delivery 的 spec_to_prompt)。
    pub fn to_prompt(&self) -> String {
        let mut p = String::new();
        if let Some(instr) = &self.instructions {
            p.push_str(&format!("# Behavior (skills)\n{instr}\n\n"));
        }
        p.push_str(&format!("# Task: {}\n\n{}\n", self.title, self.description));
        if !self.acceptance_criteria.is_empty() {
            p.push_str("\nAcceptance criteria:\n");
            for c in &self.acceptance_criteria {
                p.push_str(&format!("- {c}\n"));
            }
        }
        if let Some(ctx) = &self.context {
            if ctx != "design" {
                p.push_str(&format!("\nContext: {ctx}\n"));
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(json: &str) -> WorkSpec {
        serde_json::from_str(json).expect("parse")
    }

    #[test]
    fn deserializes_claim_and_detects_design() {
        let s = spec(r#"{"attemptId":"a1","title":"登录","description":"做","executor":"CLAUDE_CODE","context":"design","acceptanceCriteria":["c1"]}"#);
        assert_eq!(s.attempt_id, "a1");
        assert!(s.is_design());
        let p = s.to_prompt();
        assert!(p.contains("# Task: 登录"));
        assert!(p.contains("- c1"));
        assert!(!p.contains("Context: design")); // design 标记不进 prompt
    }

    #[test]
    fn implement_spec_is_not_design() {
        let s = spec(r#"{"attemptId":"a2","executor":"CODEX"}"#);
        assert!(!s.is_design());
    }
}
