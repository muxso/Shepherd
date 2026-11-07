use super::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorIdentity {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorIdentities {
    pub identities: Vec<AuthorIdentity>,
}

impl AuthorIdentities {
    pub fn normalize(mut self) -> Result<Self, Error> {
        if self.identities.len() > 500 {
            return Err(Error::Invalid("at most 500 author identities are allowed".into()));
        }
        let mut names = BTreeSet::new();
        for identity in &mut self.identities {
            identity.name = bounded(&identity.name)?;
            identity.group = identity
                .group
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(bounded)
                .transpose()?;
            if identity.aliases.len() > 50 {
                return Err(Error::Invalid("at most 50 aliases per person are allowed".into()));
            }
            if !names.insert(identity.name.clone()) {
                return Err(Error::Conflict("duplicate canonical author name".into()));
            }
        }
        let mut owners = names;
        for identity in &mut self.identities {
            let mut aliases = BTreeSet::new();
            for alias in &identity.aliases {
                let alias = bounded(alias)?;
                if alias == identity.name || !aliases.insert(alias.clone()) {
                    continue;
                }
                if !owners.insert(alias) {
                    return Err(Error::Conflict(
                        "an author alias belongs to more than one person".into(),
                    ));
                }
            }
            identity.aliases = aliases.into_iter().collect();
        }
        self.identities.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(self)
    }
}

pub(super) fn bounded(value: &str) -> Result<String, Error> {
    let value = value.trim();
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "names, groups and aliases must contain 1..255 bytes without control characters".into(),
        ));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn config(value: serde_json::Value) -> AuthorIdentities {
        serde_json::from_value(json!({"identities":value})).expect("config")
    }
    #[test]
    fn normalizes_multiple_people_and_aliases_without_guessing_case() {
        let result = config(json!([
            {"name":" Person A ","group":" Group ","aliases":["git-a","git-b","git-a","Person A"]},
            {"name":"Person B","aliases":["Git-A"]}
        ]))
        .normalize()
        .expect("valid config");
        assert_eq!(result.identities[0].name, "Person A");
        assert_eq!(result.identities[0].group.as_deref(), Some("Group"));
        assert_eq!(result.identities[0].aliases, vec!["git-a", "git-b"]);
        assert!(config(json!([])).normalize().is_ok());
    }
    #[test]
    fn rejects_ambiguous_aliases_canonical_names_and_chains() {
        for input in [
            json!([{"name":"A","aliases":["same"]},{"name":"B","aliases":[" same "]}]),
            json!([{"name":"A","aliases":["B"]},{"name":"B","aliases":[]}]),
            json!([{"name":"A","aliases":[]},{"name":" A ","aliases":[]}]),
            json!([{"name":" ","aliases":[]}]),
            json!([{"name":"A","aliases":[" "]}]),
            json!([{"name":"A","aliases":["a\nb"]}]),
        ] {
            assert!(config(input).normalize().is_err());
        }
    }
}
