use async_trait::async_trait;

use crate::domain::{ReportCard, Robot};

/// Upstream response of a webhook delivery (robot platforms answer 200 with an
/// error code in the body, so the body is kept for diagnostics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RobotDelivery {
    pub status: u16,
    pub body: String,
}

/// Delivers messages to a robot webhook.
#[async_trait]
pub trait RobotSender: Send + Sync {
    async fn send(&self, robot: &Robot, text: &str) -> Result<RobotDelivery, String>;

    async fn send_report(
        &self,
        robot: &Robot,
        text: &str,
        _report: &ReportCard,
    ) -> Result<RobotDelivery, String> {
        self.send(robot, text).await
    }
}
