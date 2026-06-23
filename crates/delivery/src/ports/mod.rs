//! 端口层:执行者出站端口 + 交付仓储。
pub mod agent_executor;
pub mod delivery_repository;
pub mod event_sink;
pub mod observer;

pub use agent_executor::{AgentExecutor, DispatchOutcome, ExecError, WorkSpec};
pub use delivery_repository::{
    DeliveryRepository, RepoError, TaskListFilter, TaskPage, TaskRow,
};
pub use event_sink::{EventSink, NoopEventSink};
pub use observer::DeliveryObserver;
