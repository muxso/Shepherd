use std::collections::BTreeMap;

use crate::domain::error::EnvironmentError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEnvironment {
    pub project_id: String,
    pub name: String,
    pub base_url: String,
    pub headers: Vec<(String, String)>,
    pub variables: BTreeMap<String, String>,
    pub enabled: bool,
}

impl NewEnvironment {
    pub fn new(
        project_id: &str,
        name: &str,
        base_url: &str,
        headers: Vec<(String, String)>,
        variables: BTreeMap<String, String>,
        enabled: bool,
    ) -> Result<Self, EnvironmentError> {
        if project_id.trim().is_empty() {
            return Err(EnvironmentError::EmptyProject);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(EnvironmentError::EmptyName);
        }
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        if !(base_url.is_empty()
            || base_url.starts_with("http://")
            || base_url.starts_with("https://"))
        {
            return Err(EnvironmentError::BadBaseUrl(base_url));
        }
        let mut clean_headers = Vec::with_capacity(headers.len());
        for (k, v) in headers {
            let k = k.trim();
            if k.is_empty() {
                return Err(EnvironmentError::EmptyHeaderName);
            }
            clean_headers.push((k.to_string(), v));
        }
        Ok(Self {
            project_id: project_id.to_string(),
            name: name.to_string(),
            base_url,
            headers: clean_headers,
            variables,
            enabled,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub base_url: String,
    pub headers: Vec<(String, String)>,
    pub variables: BTreeMap<String, String>,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn trims_name_and_strips_trailing_slash() {
        let e = NewEnvironment::new(
            "p1",
            "  local  ",
            "http://localhost:8088/",
            vec![("Authorization".into(), "Bearer x".into())],
            vars(),
            true,
        )
        .expect("ok");
        assert_eq!(e.name, "local");
        assert_eq!(e.base_url, "http://localhost:8088");
        assert_eq!(e.headers, vec![("Authorization".to_string(), "Bearer x".to_string())]);
    }

    #[test]
    fn empty_base_url_is_allowed() {
        let e = NewEnvironment::new("p1", "n", "  ", vec![], vars(), true).expect("ok");
        assert_eq!(e.base_url, "");
    }

    #[test]
    fn rejects_blank_project_and_name() {
        assert_eq!(
            NewEnvironment::new(" ", "n", "", vec![], vars(), true),
            Err(EnvironmentError::EmptyProject)
        );
        assert_eq!(
            NewEnvironment::new("p", "  ", "", vec![], vars(), true),
            Err(EnvironmentError::EmptyName)
        );
    }

    #[test]
    fn rejects_non_http_base_url() {
        assert_eq!(
            NewEnvironment::new("p", "n", "ftp://x", vec![], vars(), true),
            Err(EnvironmentError::BadBaseUrl("ftp://x".into()))
        );
    }

    #[test]
    fn rejects_blank_header_name() {
        assert_eq!(
            NewEnvironment::new("p", "n", "", vec![("  ".into(), "v".into())], vars(), true),
            Err(EnvironmentError::EmptyHeaderName)
        );
    }
}
