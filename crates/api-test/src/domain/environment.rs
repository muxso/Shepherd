use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedEnv {
    pub base_url: String,
    pub headers: Vec<(String, String)>,
    pub variables: BTreeMap<String, String>,
}

impl ResolvedEnv {
    pub fn is_empty(&self) -> bool {
        self.base_url.is_empty() && self.headers.is_empty() && self.variables.is_empty()
    }
}
