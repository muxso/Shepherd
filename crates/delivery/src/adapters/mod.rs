//! 适配器层。`in_memory` 仓储与 `stub` 执行者始终编译;其余 feature 门控。
pub mod in_memory;
pub mod stub;

pub use in_memory::InMemoryDeliveryRepository;
pub use stub::{EchoAgentExecutor, StubAgentExecutor, StubBehavior};

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "exec-local")]
pub mod local;
#[cfg(feature = "exec-http")]
pub mod agent_http;
