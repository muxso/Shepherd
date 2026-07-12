use crate::domain::error::{normalize_http_method, ApiDefinitionError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiProtocol {
    #[default]
    Http,
    Tcp,
    Sql,
    Dubbo,
    Grpc,
    Redis,
    WebSocket,
}

impl ApiProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiProtocol::Http => "HTTP",
            ApiProtocol::Tcp => "TCP",
            ApiProtocol::Sql => "SQL",
            ApiProtocol::Dubbo => "DUBBO",
            ApiProtocol::Grpc => "GRPC",
            ApiProtocol::Redis => "REDIS",
            ApiProtocol::WebSocket => "WEBSOCKET",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "HTTP" => Some(ApiProtocol::Http),
            "TCP" => Some(ApiProtocol::Tcp),
            "SQL" => Some(ApiProtocol::Sql),
            "DUBBO" => Some(ApiProtocol::Dubbo),
            "GRPC" => Some(ApiProtocol::Grpc),
            "REDIS" => Some(ApiProtocol::Redis),
            "WEBSOCKET" => Some(ApiProtocol::WebSocket),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiStatus {
    #[default]
    Draft,
    Debugging,
    Completed,
    Deprecated,
}

impl ApiStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiStatus::Draft => "DRAFT",
            ApiStatus::Debugging => "DEBUGGING",
            ApiStatus::Completed => "COMPLETED",
            ApiStatus::Deprecated => "DEPRECATED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "DRAFT" => Some(ApiStatus::Draft),
            "DEBUGGING" => Some(ApiStatus::Debugging),
            "COMPLETED" => Some(ApiStatus::Completed),
            "DEPRECATED" => Some(ApiStatus::Deprecated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiDefinition {
    pub project_id: String,
    pub name: String,
    pub protocol: ApiProtocol,
    pub method: String,
    pub path: String,
    pub status: ApiStatus,
    pub spec: String,
    pub created_by: String,
}

impl NewApiDefinition {
    pub fn new(
        project_id: &str,
        name: &str,
        protocol: ApiProtocol,
        method: &str,
        path: &str,
    ) -> Result<Self, ApiDefinitionError> {
        if project_id.trim().is_empty() {
            return Err(ApiDefinitionError::EmptyProject);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiDefinitionError::EmptyName);
        }
        let method = if protocol == ApiProtocol::Http {
            normalize_http_method(method)?
        } else {
            method.trim().to_string()
        };
        Ok(Self {
            project_id: project_id.to_string(),
            name: name.to_string(),
            protocol,
            method,
            path: path.to_string(),
            status: ApiStatus::Draft,
            spec: "{}".to_string(),
            created_by: String::new(),
        })
    }

    pub fn with_created_by(mut self, user_id: &str) -> Self {
        self.created_by = user_id.to_string();
        self
    }

    pub fn with_spec(mut self, spec: &str) -> Self {
        let spec = spec.trim();
        self.spec = if spec.is_empty() { "{}".to_string() } else { spec.to_string() };
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDefinition {
    pub id: String,
    pub num: i64,
    pub project_id: String,
    pub name: String,
    pub protocol: ApiProtocol,
    pub method: String,
    pub path: String,
    pub status: ApiStatus,
    pub module_id: Option<String>,
    pub spec: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiDefinitionChange {
    pub id: String,
    pub definition_id: String,
    pub action: String,
    pub detail: String,
    pub actor: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_roundtrip_and_default() {
        assert_eq!(ApiProtocol::default(), ApiProtocol::Http);
        assert_eq!(ApiProtocol::Http.as_str(), "HTTP");
        assert_eq!(ApiProtocol::parse("http"), Some(ApiProtocol::Http));
        assert_eq!(ApiProtocol::parse("Dubbo"), Some(ApiProtocol::Dubbo));
        assert_eq!(ApiProtocol::parse("grpc"), Some(ApiProtocol::Grpc));
        assert_eq!(ApiProtocol::parse("websocket"), Some(ApiProtocol::WebSocket));
        assert_eq!(ApiProtocol::parse("mqtt"), None);
    }

    #[test]
    fn status_roundtrip_and_default() {
        assert_eq!(ApiStatus::default(), ApiStatus::Draft);
        assert_eq!(ApiStatus::Completed.as_str(), "COMPLETED");
        assert_eq!(ApiStatus::parse("debugging"), Some(ApiStatus::Debugging));
        assert_eq!(ApiStatus::parse("ghost"), None);
    }

    #[test]
    fn new_definition_uppercases_method_and_defaults_draft() {
        let d =
            NewApiDefinition::new("p1", " 登录 ", ApiProtocol::Http, "get", "/login").expect("ok");
        assert_eq!(d.name, "登录");
        assert_eq!(d.method, "GET");
        assert_eq!(d.status, ApiStatus::Draft);
    }

    #[test]
    fn new_definition_rejects_blank_project() {
        assert_eq!(
            NewApiDefinition::new("  ", "x", ApiProtocol::Http, "GET", "/"),
            Err(ApiDefinitionError::EmptyProject)
        );
    }

    #[test]
    fn new_definition_rejects_blank_name() {
        assert_eq!(
            NewApiDefinition::new("p1", "  ", ApiProtocol::Http, "GET", "/"),
            Err(ApiDefinitionError::EmptyName)
        );
    }

    #[test]
    fn new_definition_rejects_unknown_http_method() {
        assert_eq!(
            NewApiDefinition::new("p1", "x", ApiProtocol::Http, "FETCH", "/"),
            Err(ApiDefinitionError::UnknownMethod("FETCH".into()))
        );
    }

    #[test]
    fn non_http_protocol_ignores_method() {
        let d = NewApiDefinition::new("p1", "查询", ApiProtocol::Sql, "", "SELECT 1").expect("ok");
        assert_eq!(d.protocol, ApiProtocol::Sql);
        assert_eq!(d.method, "");
    }
}
