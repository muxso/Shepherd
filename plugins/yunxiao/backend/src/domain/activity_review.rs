use super::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRule {
    pub enabled: bool,
    pub inactive_days: u32,
    pub active_window_days: u32,
    pub min_active_days: u32,
}
impl Default for ReviewRule {
    fn default() -> Self {
        Self { enabled: true, inactive_days: 3, active_window_days: 7, min_active_days: 2 }
    }
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ActivityReviewPolicy {
    pub code: ReviewRule,
    pub product: ReviewRule,
    pub test: ReviewRule,
}
impl ActivityReviewPolicy {
    pub fn validate(&self) -> Result<(), Error> {
        for rule in [&self.code, &self.product, &self.test] {
            if rule.inactive_days > 365
                || !(1..=365).contains(&rule.active_window_days)
                || rule.min_active_days > rule.active_window_days
            {
                return Err(Error::Invalid("review thresholds must fit within 365 days".into()));
            }
        }
        Ok(())
    }
}
