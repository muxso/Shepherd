#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecStatus {
    Prepared,
    Underway,
    Completed,
    Archived,
}

impl ExecStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecStatus::Prepared => "PREPARED",
            ExecStatus::Underway => "UNDERWAY",
            ExecStatus::Completed => "COMPLETED",
            ExecStatus::Archived => "ARCHIVED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PREPARED" => Some(ExecStatus::Prepared),
            "UNDERWAY" => Some(ExecStatus::Underway),
            "COMPLETED" => Some(ExecStatus::Completed),
            "ARCHIVED" => Some(ExecStatus::Archived),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStatus {
    Pending,
    Success,
    Error,
    FakeError,
    Block,
}

impl CaseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CaseStatus::Pending => "PENDING",
            CaseStatus::Success => "SUCCESS",
            CaseStatus::Error => "ERROR",
            CaseStatus::FakeError => "FAKE_ERROR",
            CaseStatus::Block => "BLOCK",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PENDING" => Some(CaseStatus::Pending),
            "SUCCESS" => Some(CaseStatus::Success),
            "ERROR" => Some(CaseStatus::Error),
            "FAKE_ERROR" => Some(CaseStatus::FakeError),
            "BLOCK" => Some(CaseStatus::Block),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionResult {
    pub item: String,
    pub actual: String,
    pub condition: String,
    pub expected: String,
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestInfo {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub name: String,
    pub kind: String,
    pub status: CaseStatus,
    pub latency_ms: u64,
    pub status_code: Option<i64>,
    pub assertions: Vec<AssertionResult>,
    pub children: Vec<StepResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CaseResult {
    pub latency_ms: u64,
    pub response_size: u64,
    pub status_code: Option<i64>,
    pub assertions: Vec<AssertionResult>,
    pub body: Option<String>,
    pub response_headers: Vec<(String, String)>,
    pub request: Option<RequestInfo>,
    pub steps: Vec<StepResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCase {
    pub case_id: String,
    pub name: String,
    pub status: CaseStatus,
    pub result: Option<CaseResult>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaseCounts {
    pub pending: u64,
    pub success: u64,
    pub error: u64,
    pub fake_error: u64,
    pub block: u64,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl CaseCounts {
    pub fn total(&self) -> u64 {
        self.pending + self.success + self.error + self.fake_error + self.block
    }

    pub fn executed(&self) -> u64 {
        self.total() - self.pending
    }

    fn nothing_executed(&self) -> bool {
        self.success == 0 && self.error == 0 && self.fake_error == 0 && self.block == 0
    }

    pub fn derive_status(&self, archived: bool) -> ExecStatus {
        if archived {
            return ExecStatus::Archived;
        }
        if self.nothing_executed() {
            ExecStatus::Prepared
        } else if self.pending == 0 {
            ExecStatus::Completed
        } else {
            ExecStatus::Underway
        }
    }

    pub fn pass_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            round2(self.success as f64 / total as f64)
        }
    }

    pub fn execute_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            round2(self.executed() as f64 / total as f64)
        }
    }

    pub fn is_pass(&self, threshold: f64) -> bool {
        self.pass_rate() >= threshold
    }

    pub fn add(&self, other: &CaseCounts) -> CaseCounts {
        CaseCounts {
            pending: self.pending + other.pending,
            success: self.success + other.success,
            error: self.error + other.error,
            fake_error: self.fake_error + other.fake_error,
            block: self.block + other.block,
        }
    }
}

/// 含 Underway 或「Completed+Prepared 混合」→ Underway;全 Completed → Completed;否则 Prepared。
pub fn status_by_children(children: &[ExecStatus]) -> ExecStatus {
    let has_underway = children.contains(&ExecStatus::Underway);
    let has_completed = children.contains(&ExecStatus::Completed);
    let has_prepared = children.contains(&ExecStatus::Prepared);

    if has_underway || (has_completed && has_prepared) {
        ExecStatus::Underway
    } else if has_completed {
        ExecStatus::Completed
    } else {
        ExecStatus::Prepared
    }
}

#[cfg(test)]
mod tests {
    use super::ExecStatus::*;
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn derive_status_prepared_when_nothing_executed() {
        let c = CaseCounts { pending: 5, ..Default::default() };
        assert_eq!(c.derive_status(false), Prepared);
    }

    #[test]
    fn derive_status_completed_when_no_pending() {
        let c = CaseCounts { success: 3, error: 1, ..Default::default() };
        assert_eq!(c.derive_status(false), Completed);
    }

    #[test]
    fn derive_status_underway_when_partially_executed() {
        let c = CaseCounts { pending: 2, success: 3, ..Default::default() };
        assert_eq!(c.derive_status(false), Underway);
    }

    #[test]
    fn archived_overrides_everything() {
        let c = CaseCounts { pending: 2, success: 3, ..Default::default() };
        assert_eq!(c.derive_status(true), Archived);
    }

    #[test]
    fn pass_and_execute_rate() {
        let c = CaseCounts { pending: 1, success: 2, error: 1, ..Default::default() };
        assert!(approx(c.pass_rate(), 0.5));
        assert!(approx(c.execute_rate(), 0.75));
    }

    #[test]
    fn rates_are_zero_when_empty() {
        let c = CaseCounts::default();
        assert!(approx(c.pass_rate(), 0.0));
        assert!(approx(c.execute_rate(), 0.0));
    }

    #[test]
    fn pass_rate_rounds_to_two_decimals() {
        let c = CaseCounts { success: 1, error: 2, ..Default::default() };
        assert!(approx(c.pass_rate(), 0.33));
    }

    #[test]
    fn is_pass_threshold_boundary() {
        let c = CaseCounts { success: 1, error: 1, ..Default::default() };
        assert!(c.is_pass(0.5));
        assert!(!c.is_pass(0.51));
    }

    #[test]
    fn children_all_prepared_is_prepared() {
        assert_eq!(status_by_children(&[Prepared, Prepared]), Prepared);
    }

    #[test]
    fn children_all_completed_is_completed() {
        assert_eq!(status_by_children(&[Completed, Completed]), Completed);
    }

    #[test]
    fn children_with_underway_is_underway() {
        assert_eq!(status_by_children(&[Completed, Underway, Prepared]), Underway);
    }

    #[test]
    fn children_completed_plus_prepared_is_underway() {
        assert_eq!(status_by_children(&[Completed, Prepared]), Underway);
    }

    #[test]
    fn children_empty_is_prepared() {
        assert_eq!(status_by_children(&[]), Prepared);
    }
}
