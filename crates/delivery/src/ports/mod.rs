//! 端口层:执行者出站端口 + 交付仓储。
pub mod agent_executor;
pub mod delivery_repository;
pub mod observer;

pub use agent_executor::{AgentExecutor, DispatchOutcome, ExecError, WorkSpec};
pub use delivery_repository::{DeliveryRepository, RepoError};
pub use observer::DeliveryObserver;
