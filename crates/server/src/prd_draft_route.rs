//! MRD → PRD 起草:把粘贴的原始素材(MRD/会议纪要/想法)整理成结构化需求草稿,
//! 回填「新建需求」表单——首页承诺的「MRD 自动转 PRD」的落地入口。
//! 配置了 LLM 由模型起草;否则走启发式(首行=标题,列表行=验收标准,其余=描述),
//! 保证无 LLM 环境下按钮同样可用。

use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use webauth::{AuthUser, SessionStore};

use crate::llm::{LlmPrdDrafter, PrdDraft};

#[derive(Clone)]
struct DraftState {
    drafter: Option<Arc<LlmPrdDrafter>>,
    sessions: Arc<dyn SessionStore>,
}

impl FromRef<DraftState> for Arc<dyn SessionStore> {
    fn from_ref(s: &DraftState) -> Self {
        s.sessions.clone()
    }
}

pub fn router(drafter: Option<Arc<LlmPrdDrafter>>, sessions: Arc<dyn SessionStore>) -> Router {
    Router::new()
        .route("/requirement/draft", post(draft_handler))
        .with_state(DraftState { drafter, sessions })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftBody {
    raw: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftResponse {
    title: String,
    description: String,
    acceptance_criteria: Vec<String>,
    priority: String,
    /// llm | heuristic:前端提示草稿来源。
    source: &'static str,
}

/// 启发式兜底:首个非空行 = 标题;`-`/`*`/数字编号行 = 验收标准;其余归入描述。
fn heuristic_draft(raw: &str) -> PrdDraft {
    let mut title = String::new();
    let mut criteria = Vec::new();
    let mut desc_lines = Vec::new();
    for line in raw.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let bullet = l
            .strip_prefix('-')
            .or_else(|| l.strip_prefix('*'))
            .or_else(|| l.strip_prefix('•'))
            .map(str::trim)
            .or_else(|| {
                l.split_once(['.', '、', ')'])
                    .filter(|(n, rest)| {
                        n.chars().all(|c| c.is_ascii_digit()) && !rest.trim().is_empty()
                    })
                    .map(|(_, rest)| rest.trim())
            });
        if title.is_empty() {
            title = l.to_string();
        } else if let Some(b) = bullet {
            criteria.push(b.to_string());
        } else {
            desc_lines.push(l.to_string());
        }
    }
    PrdDraft {
        title,
        description: desc_lines.join("\n"),
        acceptance_criteria: criteria,
        priority: "P2".to_string(),
    }
}

async fn draft_handler(
    user: AuthUser,
    State(st): State<DraftState>,
    Json(b): Json<DraftBody>,
) -> Response {
    if !user.can("REQUIREMENT", "ADD") {
        return (StatusCode::FORBIDDEN, "permission denied").into_response();
    }
    let raw = b.raw.trim();
    if raw.is_empty() {
        return (StatusCode::BAD_REQUEST, "raw required").into_response();
    }
    let (draft, source) = match &st.drafter {
        Some(d) => match d.draft(raw).await {
            Ok(v) => (v, "llm"),
            Err(e) => {
                tracing::warn!("PRD AI 起草失败,回落启发式: {e}");
                (heuristic_draft(raw), "heuristic")
            }
        },
        None => (heuristic_draft(raw), "heuristic"),
    };
    if draft.title.is_empty() {
        return (StatusCode::BAD_REQUEST, "无法从素材中提取需求").into_response();
    }
    let priority = match draft.priority.trim().to_ascii_uppercase().as_str() {
        p @ ("P0" | "P1" | "P2" | "P3") => p.to_string(),
        _ => "P2".to_string(),
    };
    (
        StatusCode::OK,
        Json(DraftResponse {
            title: draft.title,
            description: draft.description,
            acceptance_criteria: draft.acceptance_criteria,
            priority,
            source,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_extracts_title_bullets_and_description() {
        let raw = "支持手机号登录\n用户希望免密快捷进入。\n- 验证码 60 秒内送达\n- 错误验证码明确报错\n1. 三次失败锁定五分钟";
        let d = heuristic_draft(raw);
        assert_eq!(d.title, "支持手机号登录");
        assert_eq!(d.description, "用户希望免密快捷进入。");
        assert_eq!(
            d.acceptance_criteria,
            vec!["验证码 60 秒内送达", "错误验证码明确报错", "三次失败锁定五分钟"]
        );
    }

    #[test]
    fn heuristic_handles_prose_only() {
        let d = heuristic_draft("就一句话的想法");
        assert_eq!(d.title, "就一句话的想法");
        assert!(d.acceptance_criteria.is_empty());
    }
}
