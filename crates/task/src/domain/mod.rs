//! 领域层:任务拆分 DAG 的业务规则,零 IO。
pub mod task;

pub use task::{
    Decomposition, NewTask, Task, TaskError, TaskStatus, MAX_TITLE_LEN,
};
