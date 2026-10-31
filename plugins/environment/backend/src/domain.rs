use plugin_runtime::PluginConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StoreError {
    #[error("invalid profile or binding")]
    Invalid,
    #[error("profile or binding not found")]
    NotFound,
    #[error("profile changed or is still bound to an environment")]
    Conflict,
    #[error("plugin storage unavailable")]
    Unavailable,
    #[error("plugin storage must use an independent database")]
    HostDatabase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewProfile {
    pub plugin_id: String,
    pub name: String,
    pub config: PluginConfig,
    pub enabled: bool,
}
impl NewProfile {
    pub fn validate(&self, project_id: &str) -> Result<(), StoreError> {
        if !valid_ref(project_id)
            || !valid_ref(&self.plugin_id)
            || self.name.trim().is_empty()
            || self.name.len() > 200
            || self.config.schema_version == 0
            || serde_json::to_vec(&self.config).map_err(|_| StoreError::Invalid)?.len() > 262_144
        {
            return Err(StoreError::Invalid);
        }
        Ok(())
    }
}
pub fn valid_ref(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub project_id: String,
    pub plugin_id: String,
    pub name: String,
    pub config: PluginConfig,
    pub enabled: bool,
    pub revision: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingSlot {
    Connection,
    Pre,
    Post,
    Assert,
    Host,
    Display,
    Cleanup,
}
impl BindingSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Pre => "pre",
            Self::Post => "post",
            Self::Assert => "assert",
            Self::Host => "host",
            Self::Display => "display",
            Self::Cleanup => "cleanup",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub project_id: String,
    pub environment_id: String,
    pub profile_id: String,
    pub slot: BindingSlot,
    pub priority: i32,
    pub enabled: bool,
}
impl Binding {
    pub fn validate(&self) -> Result<(), StoreError> {
        if [&self.project_id, &self.environment_id, &self.profile_id].iter().any(|v| !valid_ref(v))
        {
            return Err(StoreError::Invalid);
        }
        Ok(())
    }
}
