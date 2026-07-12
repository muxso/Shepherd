//! MRD → PRD drafting: turns pasted raw material (MRD / meeting notes / ideas) into
//! a structured requirement draft that prefills the "new requirement" form — the
//! landing point for the "MRD auto-converts to PRD" promise on the home page.
//! With an LLM configured the model drafts it; otherwise a heuristic runs
//! (first line = title, list lines = acceptance criteria, the rest = description),
//! so the button still works in LLM-less environments.

use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
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

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DraftBody {
    raw: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct DraftResponse {
    title: String,
    description: String,
    acceptance_criteria: Vec<String>,
    priority: String,
    /// llm | heuristic: tells the frontend where the draft came from.
    source: &'static str,
}

/// Heuristic fallback: first non-empty line = title; `-`/`*`/numbered lines =
/// acceptance criteria; everything else goes into the description.
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

#[utoipa::path(
    post, path = "/requirement/draft", tag = "requirement",
    request_body = DraftBody,
    responses((status = 200, body = DraftResponse), (status = 400)),
    security(("bearer" = []))
)]
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

#[derive(OpenApi)]
#[openapi(
    paths(draft_handler),
    components(schemas(DraftBody, DraftResponse)),
    tags((name = "requirement", description = "requirements"))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
