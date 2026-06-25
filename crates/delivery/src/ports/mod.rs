//! 端口层:执行者出站端口 + 交付仓储。
pub mod agent_executor;
pub mod delivery_repository;
pub mod event_sink;
pub mod fleet_registry;
pub mod observer;
pub mod work_queue;

pub use agent_executor::{AgentExecutor, DispatchOutcome, ExecError, WorkSpec};
pub use fleet_registry::{FleetRegistry, RuntimeInfo, ONLINE_TTL_MS};
pub use work_queue::{Claimed, QueueStat, WorkQueue};
pub use delivery_repository::{
    DeliveryRepository, RepoError, TaskListFilter, TaskPage, TaskRow,
};
pub use event_sink::{EventSink, NoopEventSink};
pub use observer::DeliveryObserver;
