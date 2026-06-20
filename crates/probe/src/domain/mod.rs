//! 领域层:协议无关探测模型 + 通用断言(零 IO 纯函数)。
pub mod probe;

pub use probe::{evaluate, ProbeAssertion, ProbeOutcome, ProbeRequest, RawProbe};
