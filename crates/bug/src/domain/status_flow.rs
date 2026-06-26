use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusItem {
    pub id: String,
    pub name: String,
    pub internal: bool,
}

impl StatusItem {
    pub fn new(id: &str, name: &str, internal: bool) -> Self {
        Self { id: id.to_string(), name: name.to_string(), internal }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatusFlowGraph {
    items: BTreeMap<String, StatusItem>,
    edges: BTreeSet<(String, String)>,
}

impl StatusFlowGraph {
    // Edges referencing a missing status item are dropped (defends against dirty config).
    pub fn new(items: Vec<StatusItem>, edges: Vec<(String, String)>) -> Self {
        let items: BTreeMap<String, StatusItem> =
            items.into_iter().map(|s| (s.id.clone(), s)).collect();
        let edges = edges
            .into_iter()
            .filter(|(f, t)| items.contains_key(f) && items.contains_key(t))
            .collect();
        Self { items, edges }
    }

    pub fn default_bug_flow() -> Self {
        let items = vec![
            StatusItem::new("NEW", "新建", true),
            StatusItem::new("RESOLVED", "已解决", true),
            StatusItem::new("CLOSED", "已关闭", true),
            StatusItem::new("REOPENED", "重新打开", true),
            StatusItem::new("REJECTED", "已拒绝", true),
        ];
        let edges = vec![
            ("NEW".into(), "RESOLVED".into()),
            ("NEW".into(), "REJECTED".into()),
            ("RESOLVED".into(), "CLOSED".into()),
            ("RESOLVED".into(), "REOPENED".into()),
            ("REOPENED".into(), "RESOLVED".into()),
            ("CLOSED".into(), "REOPENED".into()),
            ("REJECTED".into(), "REOPENED".into()),
        ];
        Self::new(items, edges)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.items.contains_key(id)
    }

    pub fn item(&self, id: &str) -> Option<&StatusItem> {
        self.items.get(id)
    }

    pub fn targets(&self, from: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|(f, _)| f == from)
            .map(|(_, t)| t.clone())
            .collect()
    }

    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        self.edges.contains(&(from.to_string(), to.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bug_flow() -> StatusFlowGraph {
        StatusFlowGraph::default_bug_flow()
    }

    #[test]
    fn default_bug_flow_is_nonempty_and_wired() {
        let g = StatusFlowGraph::default_bug_flow();
        assert!(!g.is_empty());
        assert!(g.contains("NEW") && g.contains("CLOSED"));
        assert!(g.can_transition("NEW", "RESOLVED"));
        assert!(g.can_transition("RESOLVED", "CLOSED"));
        assert!(!g.can_transition("NEW", "CLOSED"));
        assert!(StatusFlowGraph::default().is_empty());
    }

    #[test]
    fn targets_returns_configured_edges() {
        let g = bug_flow();
        assert_eq!(g.targets("NEW"), vec!["REJECTED", "RESOLVED"]);
        assert_eq!(g.targets("RESOLVED"), vec!["CLOSED", "REOPENED"]);
    }

    #[test]
    fn targets_of_unknown_status_is_empty() {
        assert!(bug_flow().targets("GHOST").is_empty());
    }

    #[test]
    fn can_transition_allows_configured_edge() {
        let g = bug_flow();
        assert!(g.can_transition("NEW", "RESOLVED"));
        assert!(g.can_transition("RESOLVED", "CLOSED"));
    }

    #[test]
    fn can_transition_denies_unconfigured_edge() {
        let g = bug_flow();
        assert!(!g.can_transition("NEW", "CLOSED"));
        assert!(!g.can_transition("CLOSED", "NEW"));
    }

    #[test]
    fn self_transition_denied_without_self_edge() {
        assert!(!bug_flow().can_transition("NEW", "NEW"));
    }

    #[test]
    fn edges_referencing_unknown_items_are_dropped() {
        let g = StatusFlowGraph::new(
            vec![StatusItem::new("A", "a", false)],
            vec![("A".into(), "GHOST".into()), ("A".into(), "A".into())],
        );
        assert!(!g.can_transition("A", "GHOST"));
        assert!(g.can_transition("A", "A"));
    }
}
