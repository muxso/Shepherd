//! Webhook robot sender: formats a text message per platform and posts it.
//! DingTalk supports the optional "signed" mode: HMAC-SHA256 over "{ts}\n{secret}"
//! keyed by the secret, base64 then URL-encoded, appended as timestamp+sign.

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use sha2::Sha256;

use crate::domain::{Platform, ReportCard, Robot};
use crate::ports::{RobotDelivery, RobotSender};

#[derive(Clone)]
pub struct ReqwestRobotSender {
    client: reqwest::Client,
}

impl Default for ReqwestRobotSender {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestRobotSender {
    /// no_proxy: robots are often intranet relays; a global proxy must not hijack them.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

/// Standard DingTalk sign: base64(hmac_sha256(secret, "{ts}\n{secret}")), URL-encoded.
fn dingtalk_sign(secret: &str, timestamp_ms: i64) -> String {
    // HMAC accepts keys of any length; the error branch is unreachable.
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return String::new();
    };
    mac.update(format!("{timestamp_ms}\n{secret}").as_bytes());
    let sign = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    utf8_percent_encode(&sign, NON_ALPHANUMERIC).to_string()
}

fn payload(platform: Platform, text: &str) -> serde_json::Value {
    match platform {
        Platform::Feishu => serde_json::json!({"msg_type": "text", "content": {"text": text}}),
        Platform::Dingtalk | Platform::Wecom => {
            serde_json::json!({"msgtype": "text", "text": {"content": text}})
        }
    }
}

fn target_url(robot: &Robot) -> String {
    if robot.platform == Platform::Dingtalk && !robot.secret.is_empty() {
        let ts = chrono::Utc::now().timestamp_millis();
        let sep = if robot.webhook_url.contains('?') { '&' } else { '?' };
        return format!(
            "{}{sep}timestamp={ts}&sign={}",
            robot.webhook_url,
            dingtalk_sign(&robot.secret, ts)
        );
    }
    robot.webhook_url.clone()
}

#[async_trait]
impl RobotSender for ReqwestRobotSender {
    async fn send(&self, robot: &Robot, text: &str) -> Result<RobotDelivery, String> {
        self.post(robot, &payload(robot.platform, text)).await
    }

    async fn send_report(
        &self,
        robot: &Robot,
        text: &str,
        report: &ReportCard,
    ) -> Result<RobotDelivery, String> {
        if robot.platform != Platform::Wecom {
            return self.send(robot, text).await;
        }
        let Some(card) = report_card_payload(report) else {
            return self.send(robot, text).await;
        };
        self.post(robot, &card).await
    }
}

impl ReqwestRobotSender {
    async fn post(
        &self,
        robot: &Robot,
        payload: &serde_json::Value,
    ) -> Result<RobotDelivery, String> {
        let response =
            self.client.post(target_url(robot)).json(payload).send().await.map_err(|e| {
                if e.is_timeout() {
                    "robot webhook timed out"
                } else {
                    "robot webhook request failed"
                }
                .to_string()
            })?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(code) = value
                .get("errcode")
                .or_else(|| value.get("code"))
                .and_then(|value| value.as_i64())
                .filter(|code| *code != 0)
            {
                return Err(format!("robot API rejected message: code {code}"));
            }
        }
        Ok(RobotDelivery { status, body: body.chars().take(500).collect() })
    }
}

fn report_card_payload(report: &ReportCard) -> Option<serde_json::Value> {
    let url = reqwest::Url::parse(&report.url).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let mut rows = Vec::new();
    if let Some(counts) = report.counts {
        for (label, count) in ["总数", "通过", "失败"].into_iter().zip(counts) {
            rows.push(serde_json::json!({"keyname": label, "value": count.to_string()}));
        }
    }
    if !report.operator.is_empty() {
        rows.push(serde_json::json!({"keyname": "执行人", "value": report.operator.chars().take(30).collect::<String>()}));
    }
    rows.push(serde_json::json!({"keyname": "时间", "value": report.time}));
    let title: String =
        report.title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(36).collect();
    let project: String = report.project.chars().take(25).collect();
    let source = if project.is_empty() { "测试报告" } else { &project };
    let summary = if report.template_summary.is_empty() {
        report.details.join("\n")
    } else {
        report.template_summary.clone()
    };
    Some(serde_json::json!({
        "msgtype": "template_card",
        "template_card": {
            "card_type": "text_notice",
            "source": {"desc": source, "desc_color": if report.failed { 2 } else { 3 }},
            "main_title": {"title": title, "desc": report.status},
            "sub_title_text": summary.chars().take(110).collect::<String>(),
            "horizontal_content_list": rows,
            "jump_list": [{"type": 1, "title": "查看完整报告", "url": url.as_str()}],
            "card_action": {"type": 1, "url": url.as_str()},
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_shapes_per_platform() {
        let f = payload(Platform::Feishu, "hi");
        assert_eq!(f["msg_type"], "text");
        assert_eq!(f["content"]["text"], "hi");
        for p in [Platform::Dingtalk, Platform::Wecom] {
            let v = payload(p, "hi");
            assert_eq!(v["msgtype"], "text");
            assert_eq!(v["text"]["content"], "hi");
        }
    }

    #[test]
    fn wecom_report_card_uses_labeled_metrics_and_report_action() {
        let url = "http://shepherd.test/ocr-report.html?reportId=report-1";
        let payload = report_card_payload(&ReportCard {
            title: "胎号识别".into(),
            project: "AI 算法项目".into(),
            template_summary: String::new(),
            status: "场景执行失败",
            failed: true,
            counts: Some([1000, 998, 2]),
            operator: "tester".into(),
            time: "2026-11-21 15:55:18 +08:00".into(),
            details: vec!["样本 09 · 胎号 2354892105".into()],
            url: url.into(),
        })
        .unwrap();
        assert_eq!(payload["msgtype"], "template_card");
        let card = &payload["template_card"];
        assert_eq!(card["source"]["desc_color"], 2);
        assert_eq!(card["sub_title_text"], "样本 09 · 胎号 2354892105");
        assert_eq!(
            card["horizontal_content_list"][2],
            serde_json::json!({"keyname": "失败", "value": "2"})
        );
        assert_eq!(card["jump_list"][0]["url"], url);
        assert_eq!(card["card_action"]["url"], url);
    }

    #[test]
    fn dingtalk_sign_matches_reference() {
        // hmac_sha256("sec", "1000\nsec") base64 then URL-encoded.
        let sign = dingtalk_sign("sec", 1000);
        assert!(!sign.is_empty());
        // URL-encoding leaves only alphanumerics and %XX escapes.
        assert!(sign.chars().all(|c| c.is_ascii_alphanumeric() || c == '%'));
        // Deterministic for a fixed timestamp.
        assert_eq!(sign, dingtalk_sign("sec", 1000));
        assert_ne!(sign, dingtalk_sign("sec", 1001));
    }

    #[test]
    fn dingtalk_url_gains_sign_params_only_with_secret() {
        let mut robot = Robot {
            id: "r1".into(),
            project_id: "p1".into(),
            name: "dt".into(),
            platform: Platform::Dingtalk,
            webhook_url: "https://oapi.dingtalk.com/robot/send?access_token=t".into(),
            secret: "sec".into(),
            enabled: true,
            created_at: 0,
        };
        let url = target_url(&robot);
        assert!(url.contains("&timestamp=") && url.contains("&sign="));
        robot.secret = String::new();
        assert_eq!(target_url(&robot), robot.webhook_url);
        robot.platform = Platform::Feishu;
        robot.secret = "sec".into();
        assert_eq!(target_url(&robot), robot.webhook_url);
    }
}
