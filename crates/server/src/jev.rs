//! Jev (TypeSafe System One) verification judge: a fast typed-judgment model.
//!
//! Jev answers `noul` (yes/no probability) questions about a blob of state —
//! no prose, no code generation. That makes it a drop-in, cheap alternative to
//! the LLM judge for the delivery verification gate: one `noul` per acceptance
//! criterion, all in a single HTTP call; the gate passes only when every
//! probability reaches the configured threshold (default 0.8).
//!
//! Env vars: `SHEPHERD_JEV_API_KEY` (falls back to `TYPESAFE_API_KEY`),
//! `SHEPHERD_JEV_URL` (default `https://api.typesafe.ai/v1/systemone`),
//! `SHEPHERD_JEV_MODEL` (default `jev-latest`),
//! `SHEPHERD_JEV_THRESHOLD` (default `0.8`).

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use orchestrator::ports::{DeliverableView, Judge, Verdict};

const DEFAULT_URL: &str = "https://api.typesafe.ai/v1/systemone";
const DEFAULT_MODEL: &str = "jev-latest";
const DEFAULT_THRESHOLD: f64 = 0.8;

#[derive(Deserialize)]
struct SystemOneResponse {
    #[serde(default)]
    answers: BTreeMap<String, NoulAnswer>,
}

#[derive(Deserialize)]
struct NoulAnswer {
    noul: f64,
}

pub struct JevJudge {
    client: reqwest::Client,
    url: String,
    key: String,
    model: String,
    threshold: f64,
}

impl JevJudge {
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("SHEPHERD_JEV_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::env::var("TYPESAFE_API_KEY").ok().filter(|s| !s.trim().is_empty()))?;
        let url = env_or("SHEPHERD_JEV_URL", DEFAULT_URL);
        let model = env_or("SHEPHERD_JEV_MODEL", DEFAULT_MODEL);
        let threshold = std::env::var("SHEPHERD_JEV_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|t| (0.0..=1.0).contains(t))
            .unwrap_or(DEFAULT_THRESHOLD);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Some(Self { client, url, key, model, threshold })
    }

    #[cfg(test)]
    fn new(url: impl Into<String>, key: impl Into<String>, threshold: f64) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.into(),
            key: key.into(),
            model: DEFAULT_MODEL.into(),
            threshold,
        }
    }

    fn state(criteria: &[String], deliverable: &DeliverableView) -> String {
        let criteria = criteria.iter().map(|c| format!("- {c}")).collect::<Vec<_>>().join("\n");
        format!(
            "Acceptance criteria:\n{criteria}\n\nDeliverable:\nkind: {}\nreference: {}\nsummary: {}",
            deliverable.kind, deliverable.reference, deliverable.summary
        )
    }
}

#[async_trait]
impl Judge for JevJudge {
    async fn judge(&self, criteria: &[String], deliverable: &DeliverableView) -> Verdict {
        if criteria.is_empty() {
            return Verdict { passed: true, reason: "jev: no criteria to judge".into() };
        }
        let questions: BTreeMap<String, serde_json::Value> = criteria
            .iter()
            .enumerate()
            .map(|(i, c)| {
                (
                    format!("c{i}"),
                    json!({
                        "type": "noul",
                        "instructions": format!("Does the deliverable satisfy this acceptance criterion: {c}"),
                    }),
                )
            })
            .collect();
        let body = json!({ "state": Self::state(criteria, deliverable), "model": self.model, "questions": questions });

        let resp = match self.client.post(&self.url).bearer_auth(&self.key).json(&body).send().await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => return Verdict { passed: false, reason: format!("jev HTTP {}", r.status()) },
            Err(e) => return Verdict { passed: false, reason: format!("jev unreachable: {e}") },
        };
        let parsed: SystemOneResponse = match resp.json().await {
            Ok(p) => p,
            Err(e) => {
                return Verdict {
                    passed: false,
                    reason: format!("failed to parse jev response: {e}"),
                }
            }
        };

        // Every criterion must clear the threshold; report the weakest one.
        let mut weakest = (1.0_f64, String::new());
        for (i, c) in criteria.iter().enumerate() {
            let key = format!("c{i}");
            let p = match parsed.answers.get(&key) {
                Some(a) => a.noul,
                None => {
                    return Verdict {
                        passed: false,
                        reason: format!("jev response missing answer {key}"),
                    }
                }
            };
            if p < weakest.0 {
                weakest = (p, c.clone());
            }
        }
        if weakest.0 >= self.threshold {
            Verdict {
                passed: true,
                reason: format!(
                    "jev: min noul {:.2} >= threshold {:.2}",
                    weakest.0, self.threshold
                ),
            }
        } else {
            Verdict {
                passed: false,
                reason: format!(
                    "jev: noul {:.2} < threshold {:.2} for criterion: {}",
                    weakest.0, self.threshold, weakest.1
                ),
            }
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Json, Router};

    async fn spawn(app: Router, path: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}{path}")
    }

    fn dv() -> DeliverableView {
        DeliverableView {
            kind: "DIFF".into(),
            reference: "branch:feat".into(),
            summary: "done".into(),
        }
    }

    #[tokio::test]
    async fn jev_judge_passes_when_every_criterion_clears_threshold() {
        let app = Router::new().route(
            "/v1/systemone",
            post(|| async {
                Json(json!({
                    "model": "jev-1.13.0",
                    "answers": { "c0": { "type": "noul", "noul": 0.95 }, "c1": { "type": "noul", "noul": 0.9 } },
                    "usage": { "input_tokens": 1, "output_tokens": 1 }
                }))
            }),
        );
        let url = spawn(app, "/v1/systemone").await;
        let j = JevJudge::new(url, "k", 0.8);
        let v = j.judge(&["login success".into(), "token issued".into()], &dv()).await;
        assert!(v.passed, "got: {v:?}");
        assert!(v.reason.contains("0.90"), "reason names the weakest: {}", v.reason);
    }

    #[tokio::test]
    async fn jev_judge_fails_on_criterion_below_threshold() {
        let app = Router::new().route(
            "/v1/systemone",
            post(|| async {
                Json(json!({
                    "model": "jev-1.13.0",
                    "answers": { "c0": { "type": "noul", "noul": 0.99 }, "c1": { "type": "noul", "noul": 0.5 } },
                    "usage": { "input_tokens": 1, "output_tokens": 1 }
                }))
            }),
        );
        let url = spawn(app, "/v1/systemone").await;
        let j = JevJudge::new(url, "k", 0.8);
        let v = j.judge(&["login success".into(), "token issued".into()], &dv()).await;
        assert!(!v.passed);
        assert!(
            v.reason.contains("token issued"),
            "reason names the failing criterion: {}",
            v.reason
        );
    }

    #[tokio::test]
    async fn jev_judge_passes_with_no_criteria() {
        let j = JevJudge::new("http://127.0.0.1:9", "k", 0.8);
        let v = j.judge(&[], &dv()).await;
        assert!(v.passed);
        assert_eq!(v.reason, "jev: no criteria to judge");
    }

    #[tokio::test]
    async fn jev_judge_fail_closed_on_bad_json() {
        let app = Router::new().route("/v1/systemone", post(|| async { "not json at all" }));
        let url = spawn(app, "/v1/systemone").await;
        let j = JevJudge::new(url, "k", 0.8);
        let v = j.judge(&["login success".into()], &dv()).await;
        assert!(!v.passed);
        assert!(v.reason.contains("parse"), "got: {}", v.reason);
    }
}
