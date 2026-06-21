//! 领域层:功能用例模型 + 校验(零 IO)。
pub mod case;

pub use case::{CaseError, CaseStep, FunctionalCase, NewFunctionalCase};
