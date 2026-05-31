//! 缺陷状态流图。状态项 + 有向边构成的可配置状态机。
//!
//! 对应 Java 的 StatusItem(状态)+ StatusFlow(from→to 边)。把"流转是否合法"
//! 化为图上一条边是否存在的纯查询,零 IO、可穷举测试。

use std::collections::{BTreeMap, BTreeSet};

/// 一个状态项。`internal` 标识内置状态(新建/已关闭等),配置上不可删。
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

/// 状态流图:节点(状态项)+ 有向边(允许的流转)。
#[derive(Debug, Clone, Default)]
pub struct StatusFlowGraph {
    items: BTreeMap<String, StatusItem>,
    edges: BTreeSet<(String, String)>,
}

impl StatusFlowGraph {
    /// 用状态项与流转边构造。引用了不存在状态项的边会被忽略(防御脏配置)。
    pub fn new(items: Vec<StatusItem>, edges: Vec<(String, String)>) -> Self {
        let items: BTreeMap<String, StatusItem> =
            items.into_iter().map(|s| (s.id.clone(), s)).collect();
        let edges = edges
            .into_iter()
            .filter(|(f, t)| items.contains_key(f) && items.contains_key(t))
            .collect();
        Self { items, edges }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.items.contains_key(id)
    }

    pub fn item(&self, id: &str) -> Option<&StatusItem> {
        self.items.get(id)
    }

    /// 从 `from` 出发允许到达的状态(升序、去重)。未知状态返回空。
    pub fn targets(&self, from: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|(f, _)| f == from)
            .map(|(_, t)| t.clone())
            .collect()
    }

    /// 是否允许从 `from` 流转到 `to`(存在对应边)。
    pub fn can_transition(&self, from: &str, to: &str) -> bool {
        self.edges.contains(&(from.to_string(), to.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 典型缺陷流:NEW→{RESOLVED,REJECTED},RESOLVED→{CLOSED,REOPENED},
    /// REOPENED→{RESOLVED},CLOSED→{REOPENED},REJECTED→{REOPENED}
    fn bug_flow() -> StatusFlowGraph {
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
        StatusFlowGraph::new(items, edges)
    }

    #[test]
    fn targets_returns_configured_edges() {
        let g = bug_flow();
        assert_eq!(g.targets("NEW"), vec!["REJECTED", "RESOLVED"]); // 升序
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
        assert!(!g.can_transition("NEW", "CLOSED")); // 不能跳过 RESOLVED
        assert!(!g.can_transition("CLOSED", "NEW")); // 关闭不能直接回到新建
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
        assert!(!g.can_transition("A", "GHOST")); // GHOST 不是有效状态项
        assert!(g.can_transition("A", "A")); // 自环边两端都有效,保留
    }
}
