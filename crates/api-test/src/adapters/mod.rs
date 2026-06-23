//! 适配器层。test_doubles 始终编译;各执行器 feature 门控。
pub mod test_doubles;
pub use test_doubles::{
    FakeEnvironment, FakeResourcePool, NoopDispatcher, SpyDispatcher, SpyExecutor,
};

#[cfg(feature = "local")]
pub mod local;
#[cfg(feature = "local")]
pub mod plan;
#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "pg")]
pub use pg::{BatchReportDetail, CaseResultRow, PgBatchReport, PgCaseExecutionQuery, PgEnvironment, PgResourcePoolAdmin};
#[cfg(feature = "parquet-archive")]
pub mod report_archive;
#[cfg(feature = "parquet-archive")]
pub use report_archive::ReportArchiver;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "jmeter")]
pub mod jmeter;
