use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::domain::error::ApiDefinitionError;
use crate::domain::import::{
    body_type_of, kv, path_and_query, simple_spec, status_assertions, ImportedApi,
};

pub fn parse_jmeter(xml: &str) -> Result<Vec<ImportedApi>, ApiDefinitionError> {
    // quick-xml 0.41 silently drops predefined entity references (e.g. &quot;) inside
    // text content, so JSON bodies stored as {&quot;...&quot;} come out mangled. Expand
    // the XML predefined entities in text content (not inside tags) before parsing.
    let xml = expand_xml_entities(xml);
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut out = Vec::new();
    let mut cur: Option<Sampler> = None;
    let mut cur_prop: Option<String> = None;
    let mut in_arg = false;
    let mut arg_name: Option<String> = None;
    let mut arg_value: Option<String> = None;
    let mut arg_fallback: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(ApiDefinitionError::BadImport(format!("jmeter: XML parse failed: {e}")))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"HTTPSamplerProxy" => {
                    cur = Some(Sampler { name: attr(&e, b"testname"), ..Default::default() });
                }
                b"elementProp" => {
                    if attr(&e, b"elementType").as_deref() == Some("HTTPArgument") {
                        in_arg = true;
                        arg_name = None;
                        arg_value = None;
                        arg_fallback = attr(&e, b"name");
                    }
                }
                b"stringProp" | b"boolProp" => cur_prop = attr(&e, b"name"),
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if cur.is_none() {
                    continue;
                }
                // quick-xml 0.41: decode + entity unescape replace the old BytesText::unescape.
                // A logical text node can arrive as several Text events (e.g. split around
                // entity references like &quot;), so append instead of overwriting.
                let text = e
                    .decode()
                    .ok()
                    .and_then(|c| quick_xml::escape::unescape(&c).ok().map(|u| u.into_owned()))
                    .unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let Some(prop) = cur_prop.as_deref() else { continue };
                if in_arg {
                    match prop {
                        "Argument.name" => append(&mut arg_name, &text),
                        "Argument.value" => append(&mut arg_value, &text),
                        _ => {}
                    }
                } else if let Some(s) = cur.as_mut() {
                    match prop {
                        "HTTPSampler.path" => s.path.push_str(&text),
                        "HTTPSampler.method" => s.method.push_str(&text),
                        "HTTPSampler.domain" => s.domain.push_str(&text),
                        "HTTPSampler.protocol" => s.protocol.push_str(&text),
                        "HTTPSampler.postBodyRaw" => s.post_body_raw = text.trim() == "true",
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"stringProp" | b"boolProp" => cur_prop = None,
                b"elementProp" => {
                    if in_arg {
                        if let Some(s) = cur.as_mut() {
                            let name = arg_name
                                .take()
                                .filter(|n| !n.is_empty())
                                .or_else(|| arg_fallback.take());
                            s.args.push((
                                name.unwrap_or_default(),
                                arg_value.take().unwrap_or_default(),
                            ));
                        }
                        in_arg = false;
                    }
                }
                b"HTTPSamplerProxy" => {
                    if let Some(s) = cur.take() {
                        if let Some(api) = s.into_api() {
                            out.push(api);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }

    if out.is_empty() {
        return Err(ApiDefinitionError::BadImport("jmeter: no HTTP sampler found".into()));
    }
    Ok(out)
}

#[derive(Default)]
struct Sampler {
    name: Option<String>,
    method: String,
    path: String,
    domain: String,
    protocol: String,
    post_body_raw: bool,
    args: Vec<(String, String)>,
}

impl Sampler {
    fn into_api(self) -> Option<ImportedApi> {
        if self.path.trim().is_empty() && self.domain.trim().is_empty() {
            return None;
        }
        let method = if self.method.trim().is_empty() {
            "GET".to_string()
        } else {
            self.method.to_uppercase()
        };
        let raw_path = if self.path.trim().is_empty() { "/" } else { self.path.trim() };
        let normalized = if raw_path.starts_with('/') || raw_path.contains("://") {
            raw_path.to_string()
        } else {
            format!("/{raw_path}")
        };
        let (path, url_query) = path_and_query(&normalized);

        let body_method = matches!(method.as_str(), "POST" | "PUT" | "PATCH");
        let (body_text, query): (String, Vec<(String, String)>) =
            if self.post_body_raw && body_method {
                let body = self
                    .args
                    .iter()
                    .find(|(n, _)| n.is_empty())
                    .or_else(|| self.args.first())
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                (body, url_query)
            } else {
                let mut q = url_query;
                q.extend(self.args.into_iter().filter(|(n, _)| !n.is_empty()));
                (String::new(), q)
            };

        let query_kv: Vec<_> = query.iter().map(|(k, v)| kv(k, v, "")).collect();
        let body_type = body_type_of(&body_text).to_string();
        let name = self
            .name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{method} {path}"));
        let case_body =
            if body_method && !body_text.is_empty() { Some(body_text.clone()) } else { None };

        Some(ImportedApi {
            name,
            method,
            path,
            spec: simple_spec(
                "",
                Vec::new(),
                query_kv,
                Vec::new(),
                &body_type,
                &body_text,
                Vec::new(),
            ),
            case_assertions: status_assertions(200),
            case_body,
            module: None,
        })
    }
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).and_then(|a| {
        a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|c| c.into_owned())
    })
}

fn append(opt: &mut Option<String>, s: &str) {
    match opt {
        Some(v) => v.push_str(s),
        None => *opt = Some(s.to_string()),
    }
}

/// Expands the five XML predefined entities, but only in text content (outside of
/// tags), so attribute values that legitimately contain `&quot;` etc. are left intact.
/// quick-xml 0.41 drops entity references inside text, so without this step a JSON body
/// stored as `{&quot;username&quot;:&quot;admin&quot;}` would arrive as `{username:admin}`.
fn expand_xml_entities(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut text = String::new();
    let mut in_tag = false;
    let mut quote: Option<char> = None;
    let mut chars = xml.chars().peekable();

    let flush = |out: &mut String, text: &mut String| {
        if text.is_empty() {
            return;
        }
        let expanded = std::mem::take(text)
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        out.push_str(&expanded);
    };

    for c in &mut chars {
        match c {
            '<' if quote.is_none() => {
                flush(&mut out, &mut text);
                in_tag = true;
                out.push(c);
            }
            '>' if quote.is_none() => {
                in_tag = false;
                out.push(c);
            }
            '"' | '\'' if in_tag => {
                if quote == Some(c) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(c);
                }
                out.push(c);
            }
            _ => {
                if in_tag {
                    out.push(c);
                } else {
                    text.push(c);
                }
            }
        }
    }
    flush(&mut out, &mut text);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const JMX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<jmeterTestPlan version="1.2">
  <hashTree>
    <HTTPSamplerProxy guiclass="HttpTestSampleGui" testname="login" enabled="true">
      <elementProp name="HTTPsampler.Arguments" elementType="Arguments">
        <collectionProp name="Arguments.arguments">
          <elementProp name="" elementType="HTTPArgument">
            <boolProp name="HTTPArgument.always_encode">false</boolProp>
            <stringProp name="Argument.value">{&quot;username&quot;:&quot;admin&quot;}</stringProp>
            <stringProp name="Argument.metadata">=</stringProp>
          </elementProp>
        </collectionProp>
      </elementProp>
      <stringProp name="HTTPSampler.domain">api.example.com</stringProp>
      <stringProp name="HTTPSampler.protocol">https</stringProp>
      <stringProp name="HTTPSampler.path">/login</stringProp>
      <stringProp name="HTTPSampler.method">POST</stringProp>
      <boolProp name="HTTPSampler.postBodyRaw">true</boolProp>
    </HTTPSamplerProxy>
    <HTTPSamplerProxy guiclass="HttpTestSampleGui" testname="list users" enabled="true">
      <elementProp name="HTTPsampler.Arguments" elementType="Arguments">
        <collectionProp name="Arguments.arguments">
          <elementProp name="page" elementType="HTTPArgument">
            <stringProp name="Argument.value">1</stringProp>
            <stringProp name="Argument.name">page</stringProp>
          </elementProp>
        </collectionProp>
      </elementProp>
      <stringProp name="HTTPSampler.path">/users</stringProp>
      <stringProp name="HTTPSampler.method">GET</stringProp>
    </HTTPSamplerProxy>
  </hashTree>
</jmeterTestPlan>"#;

    #[test]
    fn parses_samplers_with_body_and_query() {
        let apis = parse_jmeter(JMX).expect("parsed");
        assert_eq!(apis.len(), 2);

        let login = apis.iter().find(|a| a.path == "/login").expect("login");
        assert_eq!(login.method, "POST");
        assert_eq!(login.name, "login");
        assert_eq!(login.spec["bodyType"], "json");
        assert!(login.case_body.as_deref().unwrap().contains("admin"));
        assert_eq!(login.spec["requestQuery"].as_array().unwrap().len(), 0);

        let users = apis.iter().find(|a| a.path == "/users").expect("users");
        assert_eq!(users.method, "GET");
        assert_eq!(users.spec["requestQuery"][0]["name"], "page");
        assert_eq!(users.spec["requestQuery"][0]["value"], "1");
        assert!(users.case_body.is_none());
    }

    #[test]
    fn no_sampler_is_bad_import() {
        let err = parse_jmeter("<jmeterTestPlan><hashTree/></jmeterTestPlan>").unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }

    #[test]
    fn invalid_xml_is_bad_import() {
        let err = parse_jmeter("<not closed").unwrap_err();
        assert!(matches!(err, ApiDefinitionError::BadImport(_)));
    }
}
