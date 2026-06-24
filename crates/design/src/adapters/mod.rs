//! 适配器层。`in_memory` 始终编译;`http`/`agent` feature 门控。
pub mod in_memory;
pub use in_memory::InMemoryProposalRepository;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "agent")]
pub mod delivery_drafter;
#[cfg(feature = "agent")]
pub use delivery_drafter::DeliveryDesignDrafter;
