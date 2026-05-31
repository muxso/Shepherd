//! 应用层:完整性验证用例编排。依赖 `ports` trait,不碰具体 IO。
pub mod create_verification;
pub mod verification_admin;

pub use create_verification::{CreateVerificationError, CreateVerificationUseCase};
pub use verification_admin::{VerificationCmdError, VerificationService};
