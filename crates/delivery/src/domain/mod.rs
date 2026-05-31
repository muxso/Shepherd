//! 领域层:交付尝试的业务规则,零 IO。
pub mod delivery;

pub use delivery::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, DeliveryError, ExecutorKind,
};
