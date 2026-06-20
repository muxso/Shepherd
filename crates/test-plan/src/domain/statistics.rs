//! 执行统计:用例计数 → 状态/通过率/执行率,以及计划组状态推导。纯函数。

/// 计划展示状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecStatus {
    Prepared,  // 未开始
    Underway,  // 进行中
    Completed, // 已完成
    Archived,  // 已归档
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

/// 计划内单个用例的执行结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStatus {
    Pending,   // 待执行(刚挂入计划)
    Success,   // 通过
    Error,     // 失败
    FakeError, // 误报(已知问题/非真失败)
    Block,     // 阻塞
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

/// 单条断言的判定结果(供报告逐条展示:断言项/返回值/匹配条件/匹配值/状态/原因)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionResult {
    pub item: String,      // 断言项,如「状态码」
    pub actual: String,    // 返回值(实际)
    pub condition: String, // 匹配条件,如「等于/包含」
    pub expected: String,  // 匹配值(期望)
    pub passed: bool,      // 是否通过
    pub reason: String,    // 失败原因(通过则空)
}

/// 实际请求信息(供报告「实际请求」面板)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestInfo {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// 场景嵌套步骤结果(供报告步骤树:接口用例/接口定义/控制器/脚本 等;可递归含子步骤)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub name: String,
    pub kind: String, // 接口用例 / 接口定义 / 循环控制器 / 条件控制器 / 等待 …
    pub status: CaseStatus,
    pub latency_ms: u64,
    pub status_code: Option<i64>,
    pub assertions: Vec<AssertionResult>,
    pub children: Vec<StepResult>,
}

/// 一条用例的执行明细(供报告展开:耗时/大小/状态码/断言/响应体/响应头/实际请求/步骤树)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CaseResult {
    pub latency_ms: u64,
    pub response_size: u64,
    pub status_code: Option<i64>,
    pub assertions: Vec<AssertionResult>,
    pub body: Option<String>,
    /// 响应头(供「响应头」面板)。
    pub response_headers: Vec<(String, String)>,
    /// 实际请求(供「实际请求」面板)。
    pub request: Option<RequestInfo>,
    /// 场景嵌套步骤(非场景用例则空)。
    pub steps: Vec<StepResult>,
}

/// 计划内一条用例:id + 名称 + 当前状态 + 执行明细(未执行则 None)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCase {
    pub case_id: String,
    pub name: String,
    pub status: CaseStatus,
    pub result: Option<CaseResult>,
}

/// 一个计划的用例执行结果计数。
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

    /// 已执行数 = 总数 − 待执行。
    pub fn executed(&self) -> u64 {
        self.total() - self.pending
    }

    /// 是否一条都没执行过。
    fn nothing_executed(&self) -> bool {
        self.success == 0 && self.error == 0 && self.fake_error == 0 && self.block == 0
    }

    /// 推导计划状态(已归档则恒为 Archived)。
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

    /// 通过率 = success / total(2 位小数;total 为 0 时为 0)。
    pub fn pass_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            round2(self.success as f64 / total as f64)
        }
    }

    /// 执行率 = (total − pending) / total。
    pub fn execute_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            round2(self.executed() as f64 / total as f64)
        }
    }

    /// 是否达到通过阈值(通过率 ≥ 阈值)。
    pub fn is_pass(&self, threshold: f64) -> bool {
        self.pass_rate() >= threshold
    }

    /// 累加(用于计划组聚合子计划计数)。
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

/// 计划组状态由子计划状态推导:
/// 含 Underway → Underway;Completed 与 Prepared 混合 → Underway;
/// 全 Completed → Completed;否则(全 Prepared / 空)→ Prepared。
pub fn status_by_children(children: &[ExecStatus]) -> ExecStatus {
    let has_underway = children.contains(&ExecStatus::Underway);
    let has_completed = children.contains(&ExecStatus::Completed);
    let has_prepared = children.contains(&ExecStatus::Prepared);

    // 含进行中,或"已完成 + 未开始"混合 → 进行中
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
        // 4 总数:2 成功, 1 失败, 1 待执行
        let c = CaseCounts { pending: 1, success: 2, error: 1, ..Default::default() };
        assert!(approx(c.pass_rate(), 0.5)); // 2/4
        assert!(approx(c.execute_rate(), 0.75)); // (4-1)/4
    }

    #[test]
    fn rates_are_zero_when_empty() {
        let c = CaseCounts::default();
        assert!(approx(c.pass_rate(), 0.0));
        assert!(approx(c.execute_rate(), 0.0));
    }

    #[test]
    fn pass_rate_rounds_to_two_decimals() {
        let c = CaseCounts { success: 1, error: 2, ..Default::default() }; // 1/3
        assert!(approx(c.pass_rate(), 0.33));
    }

    #[test]
    fn is_pass_threshold_boundary() {
        let c = CaseCounts { success: 1, error: 1, ..Default::default() }; // 0.5
        assert!(c.is_pass(0.5)); // 等于阈值也算通过
        assert!(!c.is_pass(0.51));
    }

    // ---- 计划组状态推导 ----
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
