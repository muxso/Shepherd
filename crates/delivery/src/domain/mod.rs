//! 领域层:交付尝试的业务规则,零 IO。
pub mod delivery;
pub mod event;

pub use delivery::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, DeliveryError, ExecutorKind,
};
pub use event::{EventError, EventKind, ExecutionEvent, NewExecutionEvent};
