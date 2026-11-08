use chrono::{Datelike, Duration, FixedOffset, Months, NaiveDate, TimeZone, Utc};
use rag::{
    domain::{RagError, Result},
    grounding::{GroundedAnswer, QueryContext},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Intent {
    pub role: &'static str,
    pub metric: &'static str,
    pub event_kind: &'static str,
    chart_kind: &'static str,
    chart: bool,
    groups: bool,
}
impl Intent {
    pub fn audit() -> Self {
        Self {
            role: "code",
            metric: "events",
            event_kind: "commit",
            chart_kind: "bar",
            chart: false,
            groups: false,
        }
    }
    pub fn parse(question: &str) -> Option<Self> {
        let q = question.to_lowercase();
        if ["怎么实现", "如何实现", "原理", "怎么配置", "如何配置", "如何安装"]
            .iter()
            .any(|word| q.contains(word))
        {
            return None;
        }
        let source = ["云效", "git", "代码", "提交", "产出", "bug", "缺陷", "测试", "需求"]
            .iter()
            .any(|word| q.contains(word));
        let chart =
            ["热力图", "统计图", "趋势图", "柱状图", "图表"].iter().any(|word| q.contains(word));
        let activity = [
            "干啥",
            "干了",
            "做了",
            "干什么",
            "工作",
            "摸鱼",
            "偷懒",
            "贡献",
            "提交情况",
            "提交记录",
            "有没有提交",
        ]
        .iter()
        .any(|word| q.contains(word));
        let assessment =
            ["摸鱼", "偷懒", "绩效", "干啥", "干了", "做了", "干什么", "工作量", "有没有产出"]
                .iter()
                .any(|word| q.contains(word));
        let activity_chart = chart
            && ["团队", "成员", "人员", "按组", "每个组", "热力图", "代码", "提交"]
                .iter()
                .any(|word| q.contains(word));
        if (!source && !assessment) || (!activity_chart && !activity) {
            return None;
        }
        let role = if q.contains("测试") && !q.contains("代码") {
            "test"
        } else if (q.contains("产品") || q.contains("需求"))
            && !q.contains("代码")
            && !q.contains("git")
            && !q.contains("提交")
        {
            "product"
        } else {
            "code"
        };
        Some(Self {
            role,
            event_kind: match role {
                "product" if q.contains("交付") || q.contains("完成") => "requirementDelivered",
                "product" => "requirementCreated",
                "test" if q.contains("用例") => "testcaseCreated",
                "test" => "bugReported",
                _ => "commit",
            },
            metric: if q.contains("bug") || q.contains("缺陷") {
                "bugs"
            } else if q.contains("代码量") || q.contains("行数") {
                "lines"
            } else if q.contains("仓库") {
                "repositories"
            } else if q.contains("需求数") || q.contains("关联需求") {
                "requirements"
            } else {
                "events"
            },
            chart_kind: if q.contains("热力图") { "activity_heatmap" } else { "bar" },
            chart,
            groups: ["按组", "每组", "各组", "每个组", "分组"].iter().any(|word| q.contains(word)),
        })
    }
}

pub(super) struct Window {
    pub from: NaiveDate,
    pub through: NaiveDate,
    pub days: i64,
}
pub(super) fn window(question: &str, at: i64) -> Result<Window> {
    let tz = FixedOffset::east_opt(8 * 3600)
        .ok_or_else(|| RagError::Backend("invalid timezone".into()))?;
    let today = Utc
        .timestamp_millis_opt(at)
        .single()
        .ok_or_else(|| RagError::Backend("invalid request clock".into()))?
        .with_timezone(&tz)
        .date_naive();
    let normalized = question.replace(['年', '月'], "-").replace('日', "").replace('/', "-");
    let regex = regex::Regex::new(r"\d{4}-\d{2}-\d{2}").expect("date pattern");
    let dates: Vec<_> = regex
        .find_iter(&normalized)
        .filter_map(|item| NaiveDate::parse_from_str(item.as_str(), "%Y-%m-%d").ok())
        .collect();
    let (from, through) = if let Some(first) = dates.first() {
        (*first, *dates.last().unwrap_or(first))
    } else if question.contains("上月") || question.contains("上个月") {
        let current = today.with_day(1).ok_or_else(|| RagError::Backend("invalid month".into()))?;
        let first = current
            .checked_sub_months(Months::new(1))
            .ok_or_else(|| RagError::Backend("invalid month".into()))?;
        (first, current - Duration::days(1))
    } else if question.contains("上周") {
        let end = today - Duration::days(i64::from(today.weekday().num_days_from_monday()) + 1);
        (end - Duration::days(6), end)
    } else if question.contains("本周") || question.contains("这周") {
        (today - Duration::days(i64::from(today.weekday().num_days_from_monday())), today)
    } else if question.contains("昨天") {
        (today - Duration::days(1), today - Duration::days(1))
    } else if question.contains("今天") {
        (today, today)
    } else if question.contains("本月") {
        (today.with_day(1).ok_or_else(|| RagError::Backend("invalid month".into()))?, today)
    } else if let Some(month) = regex::Regex::new(r"(\d{4})-(\d{1,2})(?:$|[^0-9])")
        .expect("month pattern")
        .captures(&normalized)
    {
        let year = month[1].parse().map_err(|_| RagError::Backend("invalid year".into()))?;
        let month = month[2].parse().map_err(|_| RagError::Backend("invalid month".into()))?;
        let first = NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| RagError::Backend("invalid month".into()))?;
        let last = first
            .checked_add_months(Months::new(1))
            .ok_or_else(|| RagError::Backend("invalid month".into()))?
            - Duration::days(1);
        (first, last.min(today))
    } else {
        if ["季度", "半年", "去年", "今年"].iter().any(|word| question.contains(word)) {
            return Err(RagError::Backend("请提供明确起止日期；当前最多查询最近 365 天。".into()));
        }
        let re = regex::Regex::new(r"(?:最近|近|过去)\s*(\d+)\s*天").expect("day range pattern");
        let days = re.captures(question).and_then(|item| item[1].parse::<i64>().ok()).unwrap_or(7);
        if !(1..=365).contains(&days) {
            return Err(RagError::Backend("查询范围必须为最近 1～365 天。".into()));
        }
        (today - Duration::days(days - 1), today)
    };
    let days = (today - from).num_days() + 1;
    if from > through || through > today || !(1..=365).contains(&days) {
        return Err(RagError::Backend("请指定最近 365 天以内的有效日期范围。".into()));
    }
    Ok(Window { from, through, days })
}

fn field<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}
fn clean(value: &str) -> String {
    value.replace(['\n', '\r', '[', ']', '`', '<', '>'], " ").chars().take(160).collect()
}

pub(super) fn answer(
    context: &QueryContext,
    intent: &Intent,
    range: &Window,
    source: &Value,
    data: &Value,
) -> Result<GroundedAnswer> {
    let raw = data["events"]
        .as_array()
        .ok_or_else(|| RagError::Backend("统计数据结构不完整，未获取可核验记录。".into()))?;
    let people: BTreeSet<_> = data["members"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|member| field(member, "name"))
        .chain(raw.iter().map(|event| field(event, "actor")))
        .filter(|name| name.chars().count() >= 2 && context.question.contains(name))
        .collect();
    let mut selected = people.clone();
    for event in raw {
        let alias = field(event, "authorRaw");
        if alias.len() >= 2 && context.question.to_lowercase().contains(&alias.to_lowercase()) {
            selected.insert(field(event, "actor"));
        }
    }
    let known_groups: BTreeSet<_> = raw
        .iter()
        .chain(data["members"].as_array().into_iter().flatten())
        .map(|event| field(event, "group"))
        .filter(|name| !name.is_empty() && *name != "待归属" && context.question.contains(name))
        .collect();
    let from = range.from.to_string();
    let through = range.through.to_string();
    if field(data, "from") > from.as_str() || field(data, "through") < through.as_str() {
        return Err(RagError::Backend(
            "统计数据覆盖日期与请求不一致，请重试；未将缺失日期按零处理。".into(),
        ));
    }
    if ["摸鱼", "偷懒", "绩效"].iter().any(|word| context.question.contains(word))
        && selected.is_empty()
    {
        return Err(RagError::Backend(
            "当前统计名单未识别所指成员，请提供姓名或已关联账号；不能将未识别人员判为无提交。"
                .into(),
        ));
    }
    let events: Vec<_> = raw
        .iter()
        .filter(|event| {
            field(event, "kind") == intent.event_kind
                && field(event, "date") >= from.as_str()
                && field(event, "date") <= through.as_str()
                && (selected.is_empty() || selected.contains(field(event, "actor")))
                && (known_groups.is_empty() || known_groups.contains(field(event, "group")))
        })
        .collect();
    let mut dates = Vec::new();
    let mut day = range.from;
    while day <= range.through {
        dates.push(day.to_string());
        day += Duration::days(1);
    }
    let mut rows: BTreeMap<(String, String), Vec<u64>> = BTreeMap::new();
    for member in data["members"].as_array().into_iter().flatten() {
        let group =
            if field(member, "group").is_empty() { "待归属" } else { field(member, "group") };
        let actor = field(member, "name");
        if actor.is_empty()
            || (!selected.is_empty() && !selected.contains(actor))
            || (!known_groups.is_empty() && !known_groups.contains(group))
        {
            continue;
        }
        let key = if intent.groups {
            (group.to_owned(), group.to_owned())
        } else {
            (actor.to_owned(), group.to_owned())
        };
        rows.entry(key).or_insert_with(|| vec![0; dates.len()]);
    }
    let mut uniques: BTreeMap<(String, String, usize), BTreeSet<String>> = BTreeMap::new();
    let mut totals: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for event in &events {
        let group =
            if field(event, "group").is_empty() { "待归属" } else { field(event, "group") };
        let actor = if field(event, "actor").is_empty() {
            "未关联账号"
        } else {
            field(event, "actor")
        };
        let key = if intent.groups {
            (group.to_owned(), group.to_owned())
        } else {
            (actor.to_owned(), group.to_owned())
        };
        let Some(index) = dates.iter().position(|date| date == field(event, "date")) else {
            continue;
        };
        let values = rows.entry(key.clone()).or_insert_with(|| vec![0; dates.len()]);
        match intent.metric {
            "lines" => {
                values[index] += event["additions"].as_u64().unwrap_or(0)
                    + event["deletions"].as_u64().unwrap_or(0)
            }
            "repositories" | "requirements" | "bugs" => {
                let mut ids = Vec::new();
                if intent.metric == "repositories" {
                    if let Some(id) = event.get("repoId").filter(|value| !value.is_null()) {
                        ids.push(id.as_str().map(str::to_owned).unwrap_or_else(|| id.to_string()));
                    } else if !field(event, "repo").is_empty() {
                        ids.push(field(event, "repo").to_owned());
                    }
                } else if intent.metric == "requirements" {
                    ids.extend(
                        event["requirements"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|row| row["externalId"].as_str().map(str::to_owned)),
                    );
                    if field(event, "kind").starts_with("requirement") {
                        ids.push(field(event, "externalId").to_owned());
                    }
                } else if intent.role == "test" && field(event, "kind") == "bugReported" {
                    ids.push(field(event, "externalId").to_owned());
                }
                for id in ids.into_iter().filter(|id| !id.is_empty()) {
                    uniques
                        .entry((key.0.clone(), key.1.clone(), index))
                        .or_default()
                        .insert(id.clone());
                    totals.entry(key.clone()).or_default().insert(id);
                }
                values[index] = uniques
                    .get(&(key.0.clone(), key.1.clone(), index))
                    .map_or(0, |items| items.len() as u64);
            }
            _ => values[index] += 1,
        }
    }
    if intent.metric == "bugs" && intent.role != "test" {
        for bug in data["bugs"].as_array().into_iter().flatten() {
            let Some(index) = dates.iter().position(|date| date == field(bug, "date")) else {
                continue;
            };
            for actor in
                bug["responsibles"].as_array().into_iter().flatten().filter_map(Value::as_str)
            {
                if !selected.is_empty() && !selected.contains(actor) {
                    continue;
                }
                let group = data["members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|member| field(member, "name") == actor)
                    .map(|member| field(member, "group"))
                    .filter(|value| !value.is_empty())
                    .unwrap_or("待归属");
                if !known_groups.is_empty() && !known_groups.contains(group) {
                    continue;
                }
                let key = if intent.groups {
                    (group.to_owned(), group.to_owned())
                } else {
                    (actor.to_owned(), group.to_owned())
                };
                let id = field(bug, "id").to_owned();
                if id.is_empty() {
                    continue;
                }
                uniques
                    .entry((key.0.clone(), key.1.clone(), index))
                    .or_default()
                    .insert(id.clone());
                totals.entry(key.clone()).or_default().insert(id);
                rows.entry(key.clone()).or_insert_with(|| vec![0; dates.len()])[index] = uniques
                    .get(&(key.0.clone(), key.1.clone(), index))
                    .map_or(0, |items| items.len() as u64);
            }
        }
    }

    let metric = match intent.metric {
        "lines" => "已知代码变动行数",
        "repositories" => "涉及仓库数",
        "requirements" => "关联需求数",
        "bugs" if intent.role == "test" => "新增报告缺陷数",
        "bugs" => "新增责任缺陷数",
        _ => match intent.role {
            "test" if intent.event_kind == "testcaseCreated" => "新增测试用例",
            "test" => "新增报告缺陷",
            "product" if intent.event_kind == "requirementDelivered" => "交付需求活动",
            "product" => "新增需求活动",
            _ => "已采集代码提交",
        },
    };
    let person = if selected.is_empty() {
        "项目全部已采集成员".into()
    } else {
        selected.iter().map(|name| clean(name)).collect::<Vec<_>>().join("、")
    };
    let missing_line_stats = if intent.metric == "lines" {
        events
            .iter()
            .filter(|event| {
                event["additions"].as_u64().is_none() || event["deletions"].as_u64().is_none()
            })
            .count()
    } else {
        0
    };
    let complete =
        data["coverage"]["complete"].as_bool().unwrap_or(false) && missing_line_stats == 0;
    let unique_metric = matches!(intent.metric, "repositories" | "requirements" | "bugs");
    let value_total = if unique_metric {
        totals.values().flat_map(|items| items.iter()).collect::<BTreeSet<_>>().len() as u64
    } else if intent.metric == "lines" {
        rows.values().flat_map(|values| values.iter()).sum()
    } else {
        events.len() as u64
    };
    let mut content=format!("**范围：{from} ～ {through}（上海时间，含首尾）**\n\n人员：{person}。同步截至：{}。\n\n{metric} **{value_total}**，查询范围覆盖 {} {}。",clean(field(data,"observedAt")),rows.len(),if intent.groups {"个分组"} else {"名成员或原始账号"});
    if unique_metric {
        content.push_str(
            "\n\n同一仓库、需求或缺陷可关联多名成员或分组，图中各行不能直接相加作为全局去重总数。",
        );
    }

    if missing_line_stats > 0 {
        content.push_str(&format!(
            "\n\n有 {missing_line_stats} 条活动缺少完整行数，当前代码行数仅为已知下限。"
        ));
    }
    if !complete {
        content.push_str(
            "\n\n采集覆盖尚不完整；以下仅代表已同步记录，不能据此认定没有工作或没有提交。",
        );
    }
    if ["摸鱼", "偷懒", "绩效"].iter().any(|word| context.question.contains(word)) {
        content.push_str("\n\nGit 记录不能用于直接判断是否摸鱼、工作时长或绩效。");
    }
    if intent.chart {
        content
            .push_str("\n\n热力图由上述实际查询记录按自然日汇总；空白仅表示该范围未观测到记录。");
    } else {
        let mut latest = events.clone();
        latest.sort_by_key(|event| std::cmp::Reverse(field(event, "at")));
        for event in latest.iter().take(12) {
            content.push_str(&format!(
                "\n\n- {} · {} · {}\n  {} · `{}`",
                clean(field(event, "date")),
                clean(field(event, "actor")),
                clean(field(event, "repo")),
                clean(field(event, "title")),
                field(event, "sha").chars().take(12).collect::<String>()
            ));
        }
        if events.len() > 12 {
            content.push_str(&format!("\n\n仅列出最近 12 条，当前范围共 {} 条。", events.len()));
        }
    }
    if super::audit::requested(context) {
        content = super::audit::report(context, &events, &from, &through);
        if !complete {
            content.push_str("\n\n采集覆盖不完整，本次核对只代表已同步记录。");
        }
    }
    let bar_values: Vec<_> = rows
        .iter()
        .map(|(key, values)| {
            (
                key.0.clone(),
                totals
                    .get(key)
                    .map_or_else(|| values.iter().sum::<u64>(), |items| items.len() as u64),
            )
        })
        .collect();
    let chart_rows: Vec<_> = rows
        .into_iter()
        .map(|((label, group), values)| json!({"label":label,"group":group,"values":values}))
        .collect();
    let charts = if intent.chart {
        let title = if intent.groups { "各组活动统计" } else { "成员活动统计" };
        if intent.chart_kind == "activity_heatmap" {
            vec![
                json!({"kind":"activity_heatmap","title":title,"metric":metric,"dates":dates,"rows":chart_rows,"from":from,"through":through,"observedAt":data["observedAt"],"coverageComplete":complete}),
            ]
        } else {
            let mut ranked = bar_values;
            ranked.sort_by_key(|row| std::cmp::Reverse(row.1));
            vec![
                json!({"kind":"bar","title":title,"metric":metric,"labels":ranked.iter().map(|row|&row.0).collect::<Vec<_>>(),"values":ranked.iter().map(|row|row.1).collect::<Vec<_>>(),"from":from,"through":through,"coverageComplete":complete}),
            ]
        }
    } else {
        Vec::new()
    };
    let url = format!(
        "/devops?view=authors&source={}&role={}",
        rag::references::component(field(source, "id")),
        intent.role
    );
    let sources = vec![
        json!({"title":"团队产出 · 原始统计页面","url":url,"content_preview":format!("{from} ～ {through}；{metric} {value_total}"),"occurred_at":through,"source_type":"live_statistics"}),
    ];
    Ok(GroundedAnswer {
        content,
        sources,
        charts,
        scope: json!({"projectId":context.project_id,"from":from,"through":through,"timezone":"Asia/Shanghai","sourceId":source["id"],"observedAt":data["observedAt"],"coverageComplete":complete,"returned":events.len(),"method":"team-activity-records"}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(question: &str) -> QueryContext {
        QueryContext {
            user: webauth::AuthUser {
                user_id: "user".into(),
                permissions: webauth::PermissionSet::from_raw(["PROJECT:READ"])
                    .expect("permissions"),
            },
            project_id: "project".into(),
            question: question.into(),
            history: Vec::new(),
            at_ms: 1794146400000,
        }
    }
    fn data() -> Value {
        json!({"from":"2026-11-02","through":"2026-11-08","observedAt":"2026-11-08T20:00:00+08:00","coverage":{"complete":false},
            "members":[{"name":"成员甲","group":"研发一组"},{"name":"成员乙","group":"产品组"}],
            "events":[{"kind":"commit","actor":"成员甲","authorRaw":"account-a","group":"研发一组","date":"2026-11-04","at":"2026-11-04T18:00:00+08:00","repo":"service-a","sha":"abc123","title":"订单修正"}]})
    }
    #[test]
    fn sunday_windows_are_calculated_before_querying() {
        let current = context("本周代码热力图");
        let week = window(&current.question, current.at_ms).expect("week");
        assert_eq!(
            (week.from.to_string(), week.through.to_string(), week.days),
            ("2026-11-02".into(), "2026-11-08".into(), 7)
        );
        let previous = window("上周代码热力图", current.at_ms).expect("previous");
        assert_eq!(
            (previous.from.to_string(), previous.through.to_string(), previous.days),
            ("2026-10-26".into(), "2026-11-01".into(), 14)
        );
    }
    #[test]
    fn group_heatmaps_include_configured_groups_without_observed_commits() {
        let request = context("云效数据按组发我，每个组的代码热力图");
        let intent = Intent::parse(&request.question).expect("statistics intent");
        let range = window(&request.question, request.at_ms).expect("range");
        let result =
            answer(&request, &intent, &range, &json!({"id":"source"}), &data()).expect("answer");
        let rows = result.charts[0]["rows"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row["label"] == "产品组"));
        assert!(result.content.contains("不能据此认定没有工作"));
    }
    #[test]
    fn person_heatmaps_use_real_counts_and_never_other_authors() {
        let request = context("统计下成员甲云本周代码热力图");
        let intent = Intent::parse(&request.question).expect("intent");
        let range = window(&request.question, request.at_ms).expect("range");
        let result =
            answer(&request, &intent, &range, &json!({"id":"source"}), &data()).expect("answer");
        assert_eq!(result.scope["returned"], 1);
        assert_eq!(result.charts[0]["rows"].as_array().expect("rows").len(), 1);
        assert_eq!(result.charts[0]["rows"][0]["label"], "成员甲");
        assert_eq!(result.charts[0]["rows"][0]["values"][2], 1);
    }
    #[test]
    fn matching_a_group_with_no_events_does_not_expand_to_the_whole_team() {
        let request = context("产品组本周代码热力图");
        let intent = Intent::parse(&request.question).expect("intent");
        let range = window(&request.question, request.at_ms).expect("range");
        let result =
            answer(&request, &intent, &range, &json!({"id":"source"}), &data()).expect("answer");
        assert_eq!(result.scope["returned"], 0);
        assert_eq!(result.charts[0]["rows"].as_array().expect("rows").len(), 1);
        assert_eq!(result.charts[0]["rows"][0]["label"], "成员乙");
    }
    #[test]
    fn assessment_questions_without_source_keywords_still_require_real_activity() {
        assert!(Intent::parse("本周成员甲摸鱼了吗").is_some());
        assert!(Intent::parse("成员甲上周干啥了").is_some());
    }
    #[test]
    fn implementation_questions_keep_document_retrieval() {
        assert!(Intent::parse("本周git变更检测如何实现").is_none());
        assert!(Intent::parse("接口调用链原理是什么").is_none());
    }
}
