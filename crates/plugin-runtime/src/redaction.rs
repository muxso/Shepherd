use crate::{PluginError, PluginOutput};
use aho_corasick::{AhoCorasick, MatchKind};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Applied at the host boundary, including plugin failures and assertion messages.
pub(crate) struct Redactor {
    matcher: AhoCorasick,
}
impl Redactor {
    pub fn new<'a>(secrets: impl Iterator<Item = &'a str>) -> Result<Self, PluginError> {
        let mut patterns = Vec::new();
        for secret in secrets.filter(|s| !s.is_empty()) {
            patterns.push(secret.to_string());
            // Binary database/Redis replies use hexadecimal JSON values.
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut hex = String::with_capacity(secret.len() * 2);
            for byte in secret.bytes() {
                hex.push(HEX[(byte >> 4) as usize] as char);
                hex.push(HEX[(byte & 15) as usize] as char);
            }
            patterns.push(hex.to_ascii_uppercase());
            patterns.push(hex);
            patterns.push(STANDARD.encode(secret));
            patterns.push(URL_SAFE_NO_PAD.encode(secret));
            patterns.push(url::form_urlencoded::byte_serialize(secret.as_bytes()).collect());
            if let Ok(mut url) = url::Url::parse("https://redacted@redaction.invalid") {
                if url.set_password(Some(secret)).is_ok() {
                    if let Some(password) = url.password() {
                        patterns.push(password.to_string());
                    }
                }
            }
        }
        patterns.sort();
        patterns.dedup();
        let matcher = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(patterns)
            .map_err(|_| PluginError::Unavailable("credential redaction".into()))?;
        Ok(Self { matcher })
    }
    pub fn text(&self, value: &str) -> String {
        let mut output = String::new();
        let mut offset = 0;
        for found in self.matcher.find_iter(value) {
            output.push_str(&value[offset..found.start()]);
            output.push_str("[REDACTED]");
            offset = found.end();
        }
        output.push_str(&value[offset..]);
        output
    }
    fn json(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.text(text),
            Value::Array(items) => items.iter_mut().for_each(|v| self.json(v)),
            Value::Object(map) => {
                let previous = std::mem::take(map);
                for (key, mut value) in previous {
                    self.json(&mut value);
                    map.insert(self.text(&key), value);
                }
            }
            _ => {}
        }
    }
    pub fn output(&self, mut output: PluginOutput) -> PluginOutput {
        self.json(&mut output.data);
        output.variables = output
            .variables
            .into_iter()
            .map(|(k, v)| (self.text(&k), self.text(&v)))
            .collect::<BTreeMap<_, _>>();
        for assertion in &mut output.assertions {
            assertion.id = self.text(&assertion.id);
            assertion.message = self.text(&assertion.message);
        }
        output
    }
    pub fn error(&self, error: PluginError) -> PluginError {
        match error {
            PluginError::Execution(v) => PluginError::Execution(self.text(&v)),
            PluginError::InvalidConfig(v) => PluginError::InvalidConfig(self.text(&v)),
            PluginError::InvalidContract(v) => PluginError::InvalidContract(self.text(&v)),
            PluginError::UnsupportedOperation(v) => {
                PluginError::UnsupportedOperation(self.text(&v))
            }
            PluginError::Unavailable(v) => PluginError::Unavailable(self.text(&v)),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hexadecimal_credentials_are_redacted_inside_binary_replies() {
        let secret = "secret";
        let redactor = Redactor::new(std::iter::once(secret)).expect("redactor");
        assert_eq!(redactor.text("ff73656372657400"), "ff[REDACTED]00");
        assert_eq!(redactor.text("FF73656372657400"), "FF[REDACTED]00");
        let redactor = Redactor::new(std::iter::once("jkl")).expect("redactor");
        assert_eq!(redactor.text("006A6B6C"), "00[REDACTED]");
    }
    #[test]
    fn overlapping_credentials_never_rescan_inserted_redaction_markers() {
        let redactor =
            Redactor::new(["abc", "REDACTED", "abcdef", "abc"].into_iter()).expect("redactor");
        assert_eq!(redactor.text("abcdef abc REDACTED"), "[REDACTED] [REDACTED] [REDACTED]");
        assert_eq!(Redactor::new([].into_iter()).expect("empty").text("ordinary"), "ordinary");
    }
}
