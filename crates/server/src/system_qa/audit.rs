use rag::grounding::QueryContext;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn requested(context: &QueryContext) -> bool {
    let q = &context.question;
    let check = ["核对", "核实", "真实性", "对应"].iter().any(|word| q.contains(word));
    check
        && (["日报", "周报", "逐条"].iter().any(|word| q.contains(word))
            || (q.lines().count() > 3 && q.contains('【')))
}
fn items(context: &QueryContext) -> Vec<String> {
    for candidate in std::iter::once(context.question.as_str()).chain(
        context
            .history
            .iter()
            .rev()
            .filter(|(role, _)| role == "user")
            .map(|(_, content)| content.as_str()),
    ) {
        if !candidate.contains('【') && candidate.lines().count() < 4 {
            continue;
        }
        let rows: Vec<_> = candidate
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.chars().count() > 7
                    && !["逐条核对", "日报真实性", "从git记录", "对应的代码"]
                        .iter()
                        .any(|word| line.contains(word))
                    && !["一、", "二、", "三、", "四、", "五、", "六、", "七、", "八、"]
                        .iter()
                        .any(|prefix| line.starts_with(prefix))
            })
            .take(40)
            .map(str::to_owned)
            .collect();
        if !rows.is_empty() {
            return rows;
        }
    }
    Vec::new()
}
fn tokens(value: &str) -> BTreeSet<String> {
    let value = value.split('【').next().unwrap_or(value).to_lowercase();
    let chars: Vec<_> =
        value.chars().filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation()).collect();
    chars
        .windows(2)
        .map(|pair| pair.iter().collect::<String>())
        .filter(|term| {
            ![
                "新增", "优化", "完成", "问题", "修复", "测试", "支持", "开发", "调整", "功能",
                "需求", "发布",
            ]
            .contains(&term.as_str())
        })
        .collect()
}
fn clean(value: &str) -> String {
    value.replace(['\n', '\r', '[', ']', '`', '<', '>'], " ").chars().take(140).collect()
}

pub fn report(context: &QueryContext, events: &[&Value], from: &str, through: &str) -> String {
    let rows = items(context);
    if rows.is_empty() {
        return "请把需要核对的日报条目以文字提供，并注明人员和日期。不能读取未发送给机器人的群历史或未解析附件。".into();
    }
    let mut references: BTreeMap<&str, (&Value, Vec<&Value>)> = BTreeMap::new();
    for event in events {
        for requirement in event["requirements"].as_array().into_iter().flatten() {
            if let Some(id) = requirement["externalId"].as_str() {
                references.entry(id).or_insert_with(|| (requirement, Vec::new())).1.push(event);
            }
        }
    }
    let mut output=format!("**日报证据定位：{from} ～ {through}（上海时间）**\n\n已读取 {} 条已采集活动及 {} 项关联需求。标题相似只作为候选，不代表已核实日报真假。发布、测试次数、协作与进度需要相应记录，不能由提交数证明。",events.len(),references.len());
    for (index, item) in rows.iter().enumerate() {
        output.push_str(&format!("\n\n**{}. {}**", index + 1, clean(item)));
        let source = tokens(item);
        let mut candidates = Vec::new();
        for (requirement, commits) in references.values() {
            let title = requirement["title"].as_str().unwrap_or_default();
            let serial = requirement["serial"].as_str().unwrap_or_default();
            let target = tokens(title);
            let common = source.intersection(&target).count();
            let overlap = common as f64 / source.union(&target).count().max(1) as f64;
            if (!serial.is_empty() && item.to_lowercase().contains(&serial.to_lowercase()))
                || (common >= 3 && overlap >= 0.12)
            {
                candidates.push((overlap, *requirement, commits));
            }
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let Some((_, requirement, commits)) = candidates.first() else {
            output.push_str(
                "\n未找到明确关联候选。请补充需求编号或提交 SHA；缺少采集证据不等于条目不真实。",
            );
            continue;
        };
        let label = format!(
            "{} {}",
            requirement["serial"].as_str().unwrap_or_default(),
            requirement["title"].as_str().unwrap_or_default()
        );
        let location = requirement["url"]
            .as_str()
            .and_then(rag::references::safe_url)
            .map(|url| format!("[{}]({url})", clean(&label)))
            .unwrap_or_else(|| clean(&label));
        output.push_str(&format!("\n候选需求：{location}。范围内关联 {} 条活动。", commits.len()));
        if let Some(commit) = commits.first() {
            let sha =
                commit["sha"].as_str().unwrap_or_default().chars().take(12).collect::<String>();
            let url = commit["url"].as_str().and_then(rag::references::safe_url);
            output.push_str(&format!(
                "\n提交示例：{} · {}。发布和测试完成状态未核验。",
                clean(commit["repo"].as_str().unwrap_or_default()),
                url.map(|url| format!("[{sha}]({url})")).unwrap_or(sha)
            ));
        }
    }
    output
}
