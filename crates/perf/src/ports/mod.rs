//! 端口层:单次请求执行 + 样本下沉抽象。
pub mod request_executor;
pub mod sample_sink;

pub use request_executor::RequestExecutor;
pub use sample_sink::{SampleSink, SinkError};
