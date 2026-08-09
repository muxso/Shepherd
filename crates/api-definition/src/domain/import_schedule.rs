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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSchedule {
    pub id: String,
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
    pub last_run_at: Option<String>,
    pub last_result: String,
    pub last_run_by: String,
    pub created_by: String,
    pub created_at: String,
}

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
            name: " daily sync ".into(),
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
        assert_eq!(s.format, "postman");
        assert_eq!(s.module_id, None);
        assert_eq!(s.name, "daily sync");
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
