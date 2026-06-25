//! 定时导入计划(零 IO):给一个 URL 来源配 cron,后台调度器到点拉取并按格式导入接口。
//!
//! 复用接口导入的全部选项(目标模块 / 按标签分组 / 覆盖模式 / 同步目录),外加来源 URL、
//! 鉴权(Bearer/Basic token)与 cron。构造即校验:项目 / URL / cron 非空,format 规范化。

use thiserror::Error;

use crate::domain::ImportFormat;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportScheduleError {
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("source url must not be empty")]
    EmptyUrl,
    #[error("cron expression must not be empty")]
    EmptyCron,
}

/// 已登记的定时导入计划。`cron` 为 6 段(秒 分 时 日 月 周)表达式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSchedule {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// 规范化来源格式:openapi/postman/har/jmeter/metersphere。
    pub format: String,
    pub source_url: String,
    /// 鉴权 token(明文存储,作为 Authorization 头);为空则不带鉴权。
    pub auth_token: String,
    /// token 走 Basic(true)还是 Bearer(false)。
    pub basic_auth: bool,
    pub module_id: Option<String>,
    pub group_by_tag: bool,
    pub overwrite: bool,
    pub sync_module: bool,
    pub cron: String,
    pub enabled: bool,
    /// 最近一次运行时间(文本);未运行为 None。
    pub last_run_at: Option<String>,
    /// 最近一次运行结果摘要(如 "新增 3 / 覆盖 1 / 跳过 0" 或错误)。
    pub last_result: String,
    /// 最近一次运行的操作人(手动「立即执行」记触发用户;cron 自动运行为空串)。
    pub last_run_by: String,
    pub created_by: String,
    pub created_at: String,
}

/// 待登记的定时导入计划(构造后调用 [`NewImportSchedule::validate`] 校验)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportSchedule {
    pub project_id: String,
    pub name: String,
    pub format: String,
    pub source_url: String,
    pub auth_token: String,
    pub basic_auth: bool,
    pub module_id: Option<String>,
    pub group_by_tag: bool,
    pub overwrite: bool,
    pub sync_module: bool,
    pub cron: String,
    pub enabled: bool,
    pub created_by: String,
}

impl NewImportSchedule {
    /// 校验并归一:trim 项目/URL/cron(均须非空),format 规范化,空模块归 None。
    pub fn validate(mut self) -> Result<Self, ImportScheduleError> {
        self.project_id = self.project_id.trim().to_string();
        if self.project_id.is_empty() {
            return Err(ImportScheduleError::EmptyProject);
        }
        self.source_url = self.source_url.trim().to_string();
        if self.source_url.is_empty() {
            return Err(ImportScheduleError::EmptyUrl);
        }
        self.cron = self.cron.trim().to_string();
        if self.cron.is_empty() {
            return Err(ImportScheduleError::EmptyCron);
        }
        self.format = ImportFormat::from_source(&self.format).as_str().to_string();
        self.name = self.name.trim().to_string();
        self.module_id = self.module_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> NewImportSchedule {
        NewImportSchedule {
            project_id: " p1 ".into(),
            name: " 每日同步 ".into(),
            format: "Postman".into(),
            source_url: " https://h/c.json ".into(),
            auth_token: String::new(),
            basic_auth: false,
            module_id: Some("  ".into()),
            group_by_tag: true,
            overwrite: true,
            sync_module: false,
            cron: " 0 0 2 * * * ".into(),
            enabled: true,
            created_by: "admin".into(),
        }
    }

    #[test]
    fn validates_and_normalizes() {
        let s = base().validate().expect("ok");
        assert_eq!(s.project_id, "p1");
        assert_eq!(s.source_url, "https://h/c.json");
        assert_eq!(s.cron, "0 0 2 * * *");
        assert_eq!(s.format, "postman"); // 规范化
        assert_eq!(s.module_id, None); // 空白模块归 None
        assert_eq!(s.name, "每日同步");
    }

    #[test]
    fn unknown_format_falls_back_to_openapi() {
        let mut n = base();
        n.format = "weird".into();
        assert_eq!(n.validate().unwrap().format, "openapi");
    }

    #[test]
    fn rejects_empty_required() {
        let mut n = base();
        n.project_id = "  ".into();
        assert_eq!(n.validate().unwrap_err(), ImportScheduleError::EmptyProject);
        let mut n = base();
        n.source_url = "".into();
        assert_eq!(n.validate().unwrap_err(), ImportScheduleError::EmptyUrl);
        let mut n = base();
        n.cron = " ".into();
        assert_eq!(n.validate().unwrap_err(), ImportScheduleError::EmptyCron);
    }
}
