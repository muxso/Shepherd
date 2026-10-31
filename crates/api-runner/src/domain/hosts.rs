//! Validated per-execution DNS overrides. Original request authority stays intact.
use std::{collections::BTreeMap, net::IpAddr};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostOverrides(BTreeMap<String, Vec<IpAddr>>);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostOverrideError;
impl std::fmt::Display for HostOverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid or conflicting HOST overrides; provide up to 128 DNS names with 1–16 distinct IP addresses each")
    }
}
impl std::error::Error for HostOverrideError {}
impl HostOverrides {
    pub fn new(addresses: BTreeMap<String, Vec<IpAddr>>) -> Result<Self, HostOverrideError> {
        if addresses.len() > 128 {
            return Err(HostOverrideError);
        }
        let mut result = BTreeMap::new();
        for (name, addresses) in addresses {
            if name.trim() != name || name.ends_with('.') {
                return Err(HostOverrideError);
            }
            let host = match url::Host::parse(&name).map_err(|_| HostOverrideError)? {
                url::Host::Domain(host) => host,
                _ => return Err(HostOverrideError),
            };
            if host.len() > 253
                || host.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
                || addresses.is_empty()
                || addresses.len() > 16
                || addresses.iter().collect::<std::collections::BTreeSet<_>>().len()
                    != addresses.len()
                || result.insert(host, addresses).is_some()
            {
                return Err(HostOverrideError);
            }
        }
        Ok(Self(result))
    }
    pub fn addresses(&self) -> &BTreeMap<String, Vec<IpAddr>> {
        &self.0
    }
    pub fn merge(&mut self, other: Self) -> Result<(), HostOverrideError> {
        if other.0.keys().any(|name| self.0.contains_key(name))
            || self.0.len() + other.0.len() > 128
        {
            return Err(HostOverrideError);
        }
        self.0.extend(other.0);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_cannot_hide_duplicate_or_non_dns_rules() {
        let ip = "127.0.0.1".parse().expect("IP");
        for name in [
            "127.0.0.1",
            "[::1]",
            "*.example.test",
            "example.test:80",
            "example.test/",
            "example.test.",
            "-bad.test",
            " bad.test",
        ] {
            assert!(HostOverrides::new([(name.into(), vec![ip])].into()).is_err(), "{name}");
        }
        assert!(HostOverrides::new(
            [("EXAMPLE.test".into(), vec![ip]), ("example.test".into(), vec![ip])].into()
        )
        .is_err());
        let hosts = HostOverrides::new(
            [("EXAMPLE.test".into(), vec![ip]), ("例子.test".into(), vec![ip])].into(),
        )
        .expect("canonical hosts");
        assert!(hosts.addresses().contains_key("example.test"));
        assert!(hosts.addresses().contains_key("xn--fsqu00a.test"));
        let mut merged = hosts.clone();
        assert!(merged.merge(hosts).is_err());
    }
}
