//! 读端口:用例执行记录(用例执行记录)的分页查询。
//!
//! 写路径(`PgCaseResultSink`)继续往 `ms_api_case_result` UPSERT;本端口只读,
//! 按 `case_id` 维度倒序(最近执行在前)分页取 per-case 明细,供前端"执行历史"展示。

use async_trait::async_trait;

use crate::ports::PortError;

/// 一条用例执行记录(读模型)。
///
/// `failures` 直接透传库里的 JSONB(失败断言数组);`executed_at` 统一为 RFC3339 字符串,
/// 调用方无需关心时区/时间类型,端口已在边界完成归一化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseExecutionRecord {
    pub report_id: String,
    pub case_id: String,
    pub outcome: String,
    pub failures: serde_json::Value,
    pub executed_at: String,
}

/// 用例执行记录读端口:按 `case_id` 计数 + 倒序分页。
#[async_trait]
pub trait CaseExecutionQueryPort: Send + Sync {
    /// 该用例累计执行次数(用于分页 total)。
    async fn count_by_case(&self, case_id: &str) -> Result<u64, PortError>;

    /// 该用例的执行记录,按 `executed_at` 倒序,窗口 `[offset, offset+limit)`。
    async fn list_by_case(
        &self,
        case_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<CaseExecutionRecord>, PortError>;
}
