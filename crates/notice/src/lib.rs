//! In-app notification context: producers (bug/case/comment/scheduler) push a
//! NewNotice fan-out (one row per receiver) through the Notifier service; the
//! message-center drawer reads them via the /notice HTTP adapter.
//! domain/application/ports do no IO; pg/http adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
