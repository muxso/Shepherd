//! 领域层:环境聚合,零 IO。
pub mod environment;
pub mod error;

pub use environment::{Environment, NewEnvironment};
pub use error::EnvironmentError;
