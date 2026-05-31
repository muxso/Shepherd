//! 领域层:完整性验证的业务规则,零 IO。
pub mod verification;

pub use verification::{
    CompletenessReport, CoverageLink, CriterionReport, CriterionStatus, Gap, GapKind,
    NewVerification, Verification, VerificationError, VerifiedCriterion,
};
