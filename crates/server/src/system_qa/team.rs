use super::{activity, SystemQueries};
use rag::{
    domain::{RagError, Result},
    grounding::{GroundedAnswer, QueryContext},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn requested(question: &str) -> bool {
    let q = question.to_lowercase();
    (q.contains("团队") || q.contains("全员"))
        && q.contains("产出")
        && ![
            "git", "代码", "提交", "仓库", "行数", "需求", "产品", "测试", "缺陷", "bug", "如何",
            "怎么", "原理", "按组", "分组",
        ]
        .iter()
        .any(|word| q.contains(word))
}

pub async fn answer(
    reader: &SystemQueries,
    context: &QueryContext,
    authorization: &str,
) -> Result<GroundedAnswer> {
    let source = reader.source(context, authorization).await?.ok_or_else(|| {
        RagError::Backend("当前项目尚未关联研发统计数据源，不能从知识片段推断团队产出。".into())
    })?;
    let id =
        source["id"].as_str().ok_or_else(|| RagError::Backend("invalid source identity".into()))?;
    let range = activity::window(&context.question, context.at_ms)?;
    let mut content = format!("**团队产出 · {} ～ {}（上海时间，含首尾）**\n\n未指定日期时默认最近 7 天。以下分别展示研发、产品和测试的已观测活动；不同角色的活动类型不同，不用于跨角色绩效排名。\n",range.from,range.through);
    let mut charts = Vec::new();
    let mut sources = Vec::new();
    let mut results = Vec::new();
    for (role, label, allowed) in [
        ("code", "研发", true),
        ("product", "产品", context.user.can("REQUIREMENT", "READ")),
        (
            "test",
            "测试",
            context.user.can("BUG", "READ") || context.user.can("FUNCTIONAL_CASE", "READ"),
        ),
    ] {
        if !allowed {
            content.push_str(&format!("\n{label}：无读取权限，未查询，不能判定为零。\n"));
            results.push(json!({"role":role,"status":"denied"}));
            continue;
        }
        let data = reader
            .get(
                &format!(
                    "/api/devops/sources/{}/team-activity?role={role}&days={}",
                    rag::references::component(id),
                    range.days
                ),
                authorization,
            )
            .await?;
        let (summary, chart, counts) = role_summary(role, label, &range, &data)?;
        content.push_str(&summary);
        charts.push(chart);
        results.push(counts);
        sources.push(json!({"title":format!("{label}团队产出 · 原始统计"),"url":format!("/devops?view=authors&source={}&role={role}",rag::references::component(id)),"source_type":"live_statistics","occurred_at":range.through.to_string()}));
    }
    content.push_str("\n热力图颜色表示已观测活动次数。同一需求的新建与交付可各产生一次活动；未设置真实姓名时保留未关联身份，不虚构产品或测试人员。空白不代表没有工作，采集不完整或权限不足不按零产出处理。\n");
    Ok(GroundedAnswer {
        content,
        charts,
        sources,
        scope: json!({"kind":"team_output_overview","projectId":context.project_id,"sourceId":id,"from":range.from.to_string(),"through":range.through.to_string(),"roles":results}),
    })
}

fn role_summary(
    role: &str,
    label: &str,
    range: &activity::Window,
    data: &Value,
) -> Result<(String, Value, Value)> {
    let raw = data["events"]
        .as_array()
        .ok_or_else(|| RagError::Backend("团队活动明细不完整，不能判定为零。".into()))?;
    let from = range.from.to_string();
    let through = range.through.to_string();
    if data["from"].as_str().is_none_or(|date| date > from.as_str())
        || data["through"].as_str().is_none_or(|date| date < through.as_str())
    {
        return Err(RagError::Backend(
            "团队活动日期覆盖不完整，请重试；未将缺失日期按零处理。".into(),
        ));
    }
    let mut dates = Vec::new();
    let mut day = range.from;
    while day <= range.through {
        dates.push(day.to_string());
        day += chrono::Duration::days(1);
    }
    let mut rows: BTreeMap<(String, String), Vec<u64>> = BTreeMap::new();
    for member in data["members"].as_array().into_iter().flatten() {
        if let Some(name) = member["name"].as_str().filter(|name| !name.is_empty()) {
            rows.entry((name.into(), member["group"].as_str().unwrap_or("待归属").into()))
                .or_insert_with(|| vec![0; dates.len()]);
        }
    }
    let kinds = match role {
        "code" => &["commit"][..],
        "product" => &["requirementCreated", "requirementDelivered"][..],
        _ => &["testcaseCreated", "bugReported"][..],
    };
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut active = BTreeSet::new();
    for event in raw {
        let kind = event["kind"].as_str().unwrap_or_default();
        if !kinds.contains(&kind) {
            continue;
        }
        let Some(index) =
            dates.iter().position(|date| Some(date.as_str()) == event["date"].as_str())
        else {
            continue;
        };
        let actor = event["actor"].as_str().filter(|name| !name.is_empty()).unwrap_or("未关联账号");
        let group = event["group"].as_str().unwrap_or("待归属");
        rows.entry((actor.into(), group.into())).or_insert_with(|| vec![0; dates.len()])[index] +=
            1;
        *counts.entry(kind).or_default() += 1;
        active.insert(actor);
    }
    let count = |kind| counts.get(kind).copied().unwrap_or_default();
    let complete = data["coverage"]["complete"].as_bool().unwrap_or(false);
    let summary = match role {
        "code" => format!("已观测提交 {} 次", count("commit")),
        "product" => format!(
            "已观测新建需求 {} 次、发布需求 {} 次",
            count("requirementCreated"),
            count("requirementDelivered")
        ),
        _ => format!(
            "已观测新建用例 {} 次、报告缺陷 {} 次（以当前账号可读取、已采集的事件为限）",
            count("testcaseCreated"),
            count("bugReported")
        ),
    };
    let content = format!(
        "\n**{label}**：{summary}，{} 个活跃身份。同步截至：{}。{}\n",
        active.len(),
        data["observedAt"].as_str().unwrap_or("未知"),
        if complete { "" } else { "采集覆盖待核验，不能据此判断零产出。" }
    );
    let rows: Vec<_> = rows
        .into_iter()
        .map(|((label, group), values)| json!({"label":label,"group":group,"values":values}))
        .collect();
    let chart = json!({"kind":"activity_heatmap","title":format!("{label} · 团队产出"),"metric":"已观测活动次数","dates":dates,"rows":rows,"from":from,"through":through,"coverageComplete":complete,"observedAt":data["observedAt"]});
    Ok((
        content,
        chart,
        json!({"role":role,"status":"ok","counts":counts,"activeIdentities":active.len(),"coverageComplete":complete}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn general_team_requests_keep_role_specific_questions_separate() {
        assert!(requested("云效研发统计，团队产出发我下"));
        assert!(requested("最近7天团队产出"));
        assert!(!requested("团队产出按代码提交统计"));
        assert!(!requested("团队产出如何实现"));
    }
    #[test]
    fn heatmaps_use_real_names_and_only_role_events_in_the_requested_dates() {
        let range = activity::window("本周", 1794146400000).unwrap();
        let data = json!({"from":"2026-11-02","through":"2026-11-08","members":[{"name":"真实成员","group":"产品组"}],"events":[{"kind":"requirementCreated","actor":"真实成员","group":"产品组","date":"2026-11-04"},{"kind":"commit","actor":"其他成员","date":"2026-11-04"},{"kind":"requirementCreated","actor":"历史成员","date":"2026-11-01"}],"coverage":{"complete":false}});
        let (text, chart, counts) = role_summary("product", "产品", &range, &data).unwrap();
        assert_eq!(counts["counts"]["requirementCreated"], 1);
        assert_eq!(chart["rows"].as_array().unwrap().len(), 1);
        assert_eq!(chart["rows"][0]["label"], "真实成员");
        assert!(text.contains("采集覆盖待核验"));
    }
}
