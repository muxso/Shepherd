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
#[cfg(feature = "exec-queue")]
pub mod fleet_registry;
#[cfg(feature = "exec-queue")]
pub mod queue;
#[cfg(feature = "exec-queue-redis")]
pub mod redis_queue;
#[cfg(feature = "exec-queue-redis")]
pub mod redis_registry;

#[cfg(feature = "exec-queue")]
pub use fleet_registry::InMemoryFleetRegistry;
#[cfg(feature = "exec-queue")]
pub use queue::{InMemoryWorkQueue, QueueAgentExecutor};
#[cfg(feature = "exec-queue-redis")]
pub use redis_queue::RedisStreamQueue;
#[cfg(feature = "exec-queue-redis")]
pub use redis_registry::RedisFleetRegistry;
