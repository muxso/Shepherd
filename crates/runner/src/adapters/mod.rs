//! 适配器层。in_memory 始终编译;pg/client/http feature 门控。
pub mod in_memory;
pub use in_memory::{
    InMemoryAgentStore, InMemoryCaseSpecSource, InMemoryExecutionStore, StubRemoteRunner,
};

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub use pg::{PgCaseSpecSource, PgExecutionStore, PgRunnerAgentStore};

#[cfg(feature = "client")]
pub mod reqwest_remote;
#[cfg(feature = "client")]
pub use reqwest_remote::ReqwestRemoteRunner;

#[cfg(feature = "http")]
pub mod http;
