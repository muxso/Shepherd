//! Per-execution preparation state; provider configuration is not persisted here.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedVariables {
    pub variables: BTreeMap<String, String>,
    pub steps: Vec<PreparationStepReport>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationStepReport {
    pub source: String,
    pub kind: String,
    pub elapsed_ms: u64,
    pub variable_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    MissingVariable(String),
    Limit,
    InvalidUrl,
}
impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVariable(name) => write!(f, "undefined preparation variable: {name}"),
            Self::Limit => f.write_str("prepared request exceeds size limit"),
            Self::InvalidUrl => {
                f.write_str("provide an absolute HTTP(S) URL or an environment base URL")
            }
        }
    }
}
impl std::error::Error for TemplateError {}
/// Single-pass substitution: inserted values are never interpreted as templates.
pub fn render(
    template: &str,
    variables: &BTreeMap<String, String>,
    max: usize,
) -> Result<String, TemplateError> {
    if template.len() > max {
        return Err(TemplateError::Limit);
    }
    let mut out = String::new();
    let mut rest = template;
    loop {
        let dollar = rest.find("${");
        let braces = rest.find("{{");
        let marker = match (dollar, braces) {
            (Some(a), Some(b)) => Some((a.min(b), a <= b)),
            (Some(a), None) => Some((a, true)),
            (None, Some(b)) => Some((b, false)),
            (None, None) => None,
        };
        let Some((start, dollar)) = marker else {
            append(&mut out, rest, max)?;
            break;
        };
        append(&mut out, &rest[..start], max)?;
        let after = &rest[start + 2..];
        let close = if dollar { "}" } else { "}}" };
        let Some(end) = after.find(close) else {
            append(&mut out, &rest[start..], max)?;
            break;
        };
        let name = after[..end].trim();
        let value = variables
            .get(name)
            .ok_or_else(|| TemplateError::MissingVariable(name.chars().take(128).collect()))?;
        append(&mut out, value, max)?;
        rest = &after[end + close.len()..];
    }
    Ok(out)
}
fn append(out: &mut String, value: &str, max: usize) -> Result<(), TemplateError> {
    if out.len().saturating_add(value.len()) > max {
        return Err(TemplateError::Limit);
    }
    out.push_str(value);
    Ok(())
}
pub fn prepare_request(
    mut spec: super::RequestSpec,
    base: Option<&str>,
    query: &[(String, String)],
    variables: &BTreeMap<String, String>,
    max: usize,
) -> Result<super::RequestSpec, TemplateError> {
    spec.url = render(&spec.url, variables, max)?;
    if !spec.url.starts_with("http://") && !spec.url.starts_with("https://") {
        let base = render(base.ok_or(TemplateError::InvalidUrl)?, variables, max)?;
        spec.url = format!("{}/{}", base.trim_end_matches('/'), spec.url.trim_start_matches('/'));
    }
    let mut url = url::Url::parse(&spec.url).map_err(|_| TemplateError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(TemplateError::InvalidUrl);
    }
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            pairs.append_pair(&render(name, variables, max)?, &render(value, variables, max)?);
        }
    }
    spec.url = url.into();
    spec.headers = spec
        .headers
        .into_iter()
        .map(|(k, v)| Ok((render(&k, variables, max)?, render(&v, variables, max)?)))
        .collect::<Result<_, TemplateError>>()?;
    spec.body = spec.body.map(|body| render(&body, variables, max)).transpose()?;
    let size = spec.url.len()
        + spec.headers.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
        + spec.body.as_ref().map_or(0, String::len);
    if size > max {
        return Err(TemplateError::Limit);
    }
    Ok(spec)
}
/// Legacy runners must fail before IO until they share the plugin preparation pipeline.
pub fn needs_plugin_preparation(processors: &[super::Processor]) -> bool {
    processors.iter().any(|p| {
        matches!(
            p,
            super::Processor::Plugin { .. }
                | super::Processor::Script { .. }
                | super::Processor::Sql { .. }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_renders_once_bounds_expansion_and_encodes_query_values() {
        let vars =
            [("value".into(), "a&b 世界".into()), ("literal".into(), "${value}".into())].into();
        assert_eq!(render("{{literal}}", &vars, 100).expect("literal"), "${value}");
        assert!(render("${value}", &vars, 1).is_err());
        assert!(render("{{missing}}", &vars, 100).is_err());
        let spec = super::super::RequestSpec {
            method: super::super::HttpMethod::Get,
            url: "/test".into(),
            headers: vec![],
            body: Some("{{value}}".into()),
        };
        let request = prepare_request(
            spec,
            Some("http://example.test"),
            &[("q".into(), "{{value}}".into())],
            &vars,
            1024,
        )
        .expect("request");
        let url = url::Url::parse(&request.url).expect("URL");
        assert_eq!(url.query_pairs().collect::<Vec<_>>(), vec![("q".into(), "a&b 世界".into())]);
        assert_eq!(request.body.as_deref(), Some("a&b 世界"));
    }
}
