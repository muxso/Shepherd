//! 协议插件实现。每种协议 feature 门控:启用哪些 = runner 支持哪些系统。
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "http")]
pub use http::HttpPlugin;

#[cfg(feature = "grpc")]
pub mod grpc;
#[cfg(feature = "grpc")]
pub use grpc::GrpcPlugin;

#[cfg(feature = "sql")]
pub mod sql;
#[cfg(feature = "sql")]
pub use sql::SqlPlugin;

#[cfg(feature = "redis")]
pub mod redis;
#[cfg(feature = "redis")]
pub use redis::RedisPlugin;
