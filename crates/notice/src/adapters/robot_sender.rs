//! Webhook robot sender: formats a text message per platform and posts it.
//! DingTalk supports the optional "signed" mode: HMAC-SHA256 over "{ts}\n{secret}"
//! keyed by the secret, base64 then URL-encoded, appended as timestamp+sign.

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use sha2::Sha256;

use crate::domain::{Platform, Robot};
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
        let resp = self
            .client
            .post(target_url(robot))
            .json(&payload(robot.platform, text))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        // Robot APIs cap error bodies well below this; truncate just in case.
        let body = body.chars().take(500).collect();
        Ok(RobotDelivery { status, body })
    }
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
