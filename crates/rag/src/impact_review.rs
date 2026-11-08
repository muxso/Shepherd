use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub source: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub category: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub line: Option<usize>,
    #[serde(default)]
    pub proposed_new_file: bool,
    #[serde(default)]
    pub confidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Opinion {
    pub verdict: String,
    pub summary: String,
    pub findings: Vec<Finding>,
}
pub fn validate(
    mut opinion: Opinion,
    evidence: &[Evidence],
    gaps: &[String],
) -> Result<Opinion, String> {
    let sources: BTreeMap<_, _> = evidence.iter().map(|e| (e.id.as_str(), e)).collect();
    if !matches!(opinion.verdict.as_str(), "APPROVE" | "REVISE" | "REJECT" | "UNSURE")
        || opinion.summary.trim().is_empty()
        || opinion.summary.len() > 16000
        || opinion.findings.len() > 100
    {
        return Err("invalid review shape".into());
    }
    for f in &mut opinion.findings {
        if !matches!(
            f.category.as_str(),
            "requirement" | "change" | "indirect" | "test" | "question"
        ) || !matches!(f.severity.as_str(), "critical" | "high" | "medium" | "low")
            || f.title.trim().is_empty()
            || f.title.len() > 1000
            || f.detail.len() > 12000
            || f.evidence_ids.is_empty()
            || f.evidence_ids.len() > 20
        {
            return Err("invalid finding".into());
        }
        let mut unique = BTreeSet::new();
        for id in &f.evidence_ids {
            if !sources.contains_key(id.as_str()) || !unique.insert(id) {
                return Err("finding cites unavailable evidence".into());
            }
        }
        if let Some(repo) = f.repository_id.as_deref() {
            if !f.evidence_ids.iter().any(|id| {
                sources.get(id.as_str()).is_some_and(|e| e.source["repositoryId"] == repo)
            }) {
                return Err("finding invents a repository".into());
            }
        }
        if f.proposed_new_file && (f.repository_id.is_none() || f.path.is_none()) {
            return Err("new file requires a repository and path".into());
        }
        if let Some(path) = f.path.as_deref() {
            if path.is_empty()
                || path.starts_with('/')
                || path.contains('\\')
                || path.split('/').any(|p| p == "..")
                || path.len() > 2000
            {
                return Err("invalid file path".into());
            }
            if !f.proposed_new_file
                && !f.evidence_ids.iter().any(|id| {
                    sources.get(id.as_str()).is_some_and(|e| {
                        e.source["path"] == path
                            && f.repository_id
                                .as_deref()
                                .is_none_or(|r| e.source["repositoryId"] == r)
                    })
                })
            {
                return Err("finding invents a file location".into());
            }
        }
        if let Some(line) = f.line {
            if f.proposed_new_file
                || !f.evidence_ids.iter().any(|id| {
                    sources.get(id.as_str()).is_some_and(|e| {
                        let start = e.source["startLine"]
                            .as_u64()
                            .or_else(|| e.source["line"].as_u64())
                            .unwrap_or(0) as usize;
                        let end = e.source["endLine"]
                            .as_u64()
                            .or_else(|| e.source["line"].as_u64())
                            .unwrap_or(0) as usize;
                        line >= start
                            && line <= end
                            && start > 0
                            && f.repository_id
                                .as_deref()
                                .is_none_or(|r| e.source["repositoryId"] == r)
                            && f.path.as_deref().is_none_or(|p| e.source["path"] == p)
                    })
                })
            {
                return Err("finding cites lines outside its evidence".into());
            }
        }
        if !matches!(f.confidence.as_str(), "high" | "medium" | "low") {
            f.confidence = "low".into();
        }
    }
    if !gaps.is_empty() && opinion.verdict == "APPROVE" {
        opinion.verdict = "UNSURE".into();
    }
    Ok(opinion)
}
pub fn parse(text: &str, evidence: &[Evidence], gaps: &[String]) -> Result<Opinion, String> {
    let json = text
        .find('{')
        .and_then(|start| text.rfind('}').filter(|end| *end >= start).map(|end| &text[start..=end]))
        .ok_or("review is not JSON")?;
    let opinion: Opinion =
        serde_json::from_str(json).map_err(|_| "review JSON does not match the evidence schema")?;
    validate(opinion, evidence, gaps)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn evidence() -> Vec<Evidence> {
        vec![Evidence {
            id: "E1".into(),
            kind: "code".into(),
            title: "Orders".into(),
            content: "L12: fn order()".into(),
            source: json!({"repositoryId":"repo","path":"src/order.rs","startLine":12,"endLine":15}),
        }]
    }
    fn opinion() -> Opinion {
        Opinion {
            verdict: "APPROVE".into(),
            summary: "Reviewed".into(),
            findings: vec![Finding {
                category: "change".into(),
                severity: "high".into(),
                title: "Validate input".into(),
                detail: "Add boundary checks".into(),
                evidence_ids: vec!["E1".into()],
                repository_id: Some("repo".into()),
                path: Some("src/order.rs".into()),
                line: Some(12),
                proposed_new_file: false,
                confidence: "high".into(),
            }],
        }
    }
    #[test]
    fn unknown_citations_repositories_paths_and_lines_fail_closed() {
        let e = evidence();
        assert!(validate(opinion(), &e, &[]).is_ok());
        let mut o = opinion();
        o.findings[0].evidence_ids = vec!["invented".into()];
        assert!(validate(o, &e, &[]).is_err());
        let mut o = opinion();
        o.findings[0].repository_id = Some("other".into());
        assert!(validate(o, &e, &[]).is_err());
        let mut o = opinion();
        o.findings[0].path = Some("src/made-up.rs".into());
        assert!(validate(o, &e, &[]).is_err());
        let mut o = opinion();
        o.findings[0].line = Some(99);
        assert!(validate(o, &e, &[]).is_err());
    }
    #[test]
    fn gaps_prevent_unqualified_approval() {
        assert_eq!(
            validate(opinion(), &evidence(), &["inventory incomplete".into()])
                .expect("valid partial opinion")
                .verdict,
            "UNSURE"
        );
        assert!(parse("not JSON", &[], &[]).is_err());
    }
}
