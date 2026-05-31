//! 领域层:评审状态机,零 IO。
pub mod review;

pub use review::{
    aggregate_status, effective_verdicts, PassRule, ReviewError, ReviewRecord, ReviewSetting,
    ReviewStatus, Verdict, SYSTEM_USER,
};
