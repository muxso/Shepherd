use super::{activity, business, SystemQueries};
use rag::{
    domain::{RagError, Result},
    grounding::{GroundedAnswer, QueryContext},
};
use serde_json::{json, Value};

pub(super) async fn answer(
    reader: &SystemQueries,
    context: &QueryContext,
    authorization: &str,
) -> Result<Option<GroundedAnswer>> {
    let q = &context.question;
    if overdue_work_requested(q) {
        if let Some(source) = reader.source(context, authorization).await? {
            return overdue_work(reader, context, authorization, &source).await.map(Some);
        }
    }
    if !q.contains("云效")
        || prefers_activity(q)
        || q.contains("热力图")
        || q.contains("团队")
        || q.contains("成员")
        || q.contains("按组")
        || rag::grounding::needs_change_proof(q)
    {
        return Ok(None);
    }
    let source = reader
        .source(context, authorization)
        .await?
        .ok_or_else(|| RagError::Backend("当前项目尚未关联云效数据源。".into()))?;
    let id =
        source["id"].as_str().ok_or_else(|| RagError::Backend("invalid source identity".into()))?;
    let encoded = rag::references::component(id);
    let w = activity::window(q, context.at_ms)?;
    let chart = q.contains('图') || q.contains("分布") || q.contains("趋势");
    let mut charts = Vec::new();
    let mut content = String::new();
    let view;
    let evidence;
    if q.contains("缺陷") || q.to_lowercase().contains("bug") {
        if !context.user.can("BUG", "READ") {
            return Err(RagError::Backend("当前账号没有读取缺陷的权限。".into()));
        }
        view = "bugs";
        let role = if q.contains("提交人") || q.contains("报告人") {
            "reporter"
        } else if q.contains("经办人") {
            "assignee"
        } else {
            "responsible"
        };
        let mut path = format!(
            "/api/devops/sources/{encoded}/bug-statistics?from={}&through={}&personRole={role}",
            w.from, w.through
        );
        let mut value = reader.get(&path, authorization).await?;
        let people: Vec<_> = value["personOptions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().or_else(|| v["name"].as_str()))
            .filter(|name| !name.is_empty() && q.contains(name))
            .map(str::to_owned)
            .collect();
        let groups: Vec<_> = value["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().or_else(|| v["name"].as_str()))
            .filter(|name| !name.is_empty() && q.contains(name))
            .map(str::to_owned)
            .collect();
        if people.len() > 1 || groups.len() > 1 {
            return Err(RagError::Backend("请指定一位人员或一个分组，或查询整体分布。".into()));
        }
        if let Some(person) = people.first() {
            path.push_str(&format!("&person={}", rag::references::component(person)));
        }
        if let Some(group) = groups.first() {
            path.push_str(&format!("&group={}", rag::references::component(group)));
        }
        if !people.is_empty() || !groups.is_empty() {
            value = reader.get(&path, authorization).await?;
        }
        let total = value["total"]
            .as_i64()
            .ok_or_else(|| RagError::Backend("缺陷统计结果不完整。".into()))?;
        content.push_str(&format!(
            "云效新增缺陷 · {} 至 {}（上海时区）\n共 {total} 条新增有效缺陷。统计角色：{role}。\n",
            w.from, w.through
        ));
        for (label, key) in [("状态", "statuses"), ("优先级", "priorities"), ("人员", "people")]
        {
            let rows: Vec<_> = value[key].as_array().into_iter().flatten().take(12).collect();
            if !rows.is_empty() {
                content.push_str(&format!(
                    "{label}：{}。\n",
                    rows.iter()
                        .map(|r| format!(
                            "{} {}",
                            r["name"].as_str().or_else(|| r["status"].as_str()).unwrap_or("未归属"),
                            r["count"]
                        ))
                        .collect::<Vec<_>>()
                        .join("；")
                ));
            }
            if chart {
                charts.push(json!({"kind":"bar","title":format!("新增有效缺陷 · 按{label}"),"labels":rows.iter().map(|r|r["name"].as_str().or_else(||r["status"].as_str()).unwrap_or("未归属")).collect::<Vec<_>>(),"values":rows.iter().map(|r|r["count"].clone()).collect::<Vec<_>>()}));
            }
        }
        evidence = json!({"total":total,"from":w.from.to_string(),"through":w.through.to_string(),"people":people,"groups":groups,"missingTime":value["missingTime"]});
    } else if q.contains("需求") || q.contains("用例") {
        let testcase = q.contains("用例");
        let permission = if testcase { "FUNCTIONAL_CASE" } else { "REQUIREMENT" };
        if !context.user.can(permission, "READ") {
            return Err(RagError::Backend("当前账号没有读取该云效业务数据的权限。".into()));
        }
        view = if testcase { "records" } else { "delivery" };
        let delayed = q.contains("延期") || q.contains("逾期");
        let period = business::explicit_time(q) && !delayed;
        let kind = if testcase { "testcase" } else { "requirement" };
        let delivered = q.contains("交付") || q.contains("已发布");
        let metric = if testcase {
            "testcases"
        } else if delivered {
            "delivered"
        } else {
            "requirements"
        };
        let mut path=format!("/api/devops/sources/{encoded}/records?kind={kind}&page=1&pageSize=12&delayedOnly={delayed}");
        if period {
            path.push_str(&format!(
                "&metric={metric}&period=true&date={}&days={}",
                w.through,
                (w.through - w.from).num_days() + 1
            ));
        }
        if let Some(term) = q.split(['“', '”', '"']).nth(1).filter(|s| !s.trim().is_empty()) {
            path.push_str(&format!("&search={}", rag::references::component(term)));
        }
        let value = reader.get(&path, authorization).await?;
        let total = value["total"]
            .as_i64()
            .ok_or_else(|| RagError::Backend("云效记录查询结果不完整。".into()))?;
        content.push_str(&format!(
            "云效{}{}：{total} 条。\n{}\n",
            if delayed {
                "当前延期"
            } else if period && delivered {
                "期间已发布"
            } else if period {
                "期间新增"
            } else {
                "当前已同步"
            },
            if testcase { "测试用例" } else { "需求" },
            if period {
                format!("时间范围：{} 至 {}（上海时区）。", w.from, w.through)
            } else {
                "这是当前已同步状态快照，不代表某个历史时段的新增或交付量。".into()
            }
        ));
        for item in value["items"].as_array().into_iter().flatten() {
            let row = if item["data"].is_object() { &item["data"] } else { item };
            content.push_str(&format!(
                "- {} {} · {} · {}\n",
                row["serial"].as_str().unwrap_or_default(),
                row["title"].as_str().unwrap_or("未命名"),
                row["status"].as_str().unwrap_or("未设置"),
                row["owner"].as_str().unwrap_or("未归属")
            ));
        }
        if total > 12 {
            content.push_str("明细仅展示最近 12 条，合计包含全部匹配记录。\n");
        }
        evidence = json!({"total":total,"period":period,"kind":kind,"metric":metric});
        if chart {
            charts.push(json!({"kind":"bar","title":"匹配记录数","labels":[if testcase{"测试用例"}else{"需求"}],"values":[total]}));
        }
    } else if ["概览", "汇总", "情况", "统计"].iter().any(|v| q.contains(v)) {
        view = "overview";
        let value = reader
            .get(&format!("/api/devops/sources/{encoded}/dashboard?days={}", w.days), authorization)
            .await?;
        let totals = &value["totals"];
        if !totals.is_object() {
            return Err(RagError::Backend("云效概览结果不完整。".into()));
        }
        content.push_str("云效当前已同步状态概览：\n");
        for (label, key) in [
            ("需求", "requirements"),
            ("缺陷", "bugs"),
            ("迭代", "sprints"),
            ("当前延期需求", "delayed"),
        ] {
            content.push_str(&format!("- {label}：{}\n", totals[key]));
        }
        content
            .push_str("以上为当前状态快照。期间新增、交付及成员活动请明确查询对象和时间范围。\n");
        evidence = totals.clone();
        if chart {
            charts.push(json!({"kind":"bar","title":"云效当前状态概览","labels":["需求","缺陷","迭代","延期需求"],"values":[totals["requirements"],totals["bugs"],totals["sprints"],totals["delayed"]]}));
        }
    } else {
        return Ok(None);
    }
    let url = format!("/devops?source={encoded}&view={view}&days={}", w.days);
    content.push_str(&format!(
        "\n[打开云效来源页面]({url})。仅统计已采集记录；未同步或缺少时间的数据不能视为零。"
    ));
    Ok(Some(GroundedAnswer {
        content,
        sources: vec![
            json!({"doc_id":format!("devops:{id}:{view}"),"title":"云效业务查询","url":url,"source_type":"business_query","content_preview":evidence.to_string()}),
        ],
        charts,
        scope: json!({"kind":"devops_query","projectId":context.project_id,"sourceId":id,"evidence":evidence}),
    }))
}

fn overdue_work_requested(q: &str) -> bool {
    ["延期", "逾期"].iter().any(|v| q.contains(v))
        && ["任务", "需求", "工作项"].iter().any(|v| q.contains(v))
        && !prefers_activity(q)
        && ![
            "如何",
            "怎么",
            "原理",
            "本地",
            "拆分",
            "研发任务",
            "开发任务",
            "接口",
            "测试计划",
            "用例",
            "缺陷",
            "bug",
        ]
        .iter()
        .any(|v| q.contains(v))
}

fn overdue_owner(q: &str, owners: &Value) -> Result<Option<String>> {
    let matched: std::collections::BTreeSet<_> = owners
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|name| name.chars().count() > 1 && q.contains(name))
        .collect();
    if matched.len() > 1 {
        return Err(RagError::Backend("请指定一位负责人查询延期工作项。".into()));
    }
    if let Some(name) = matched.first() {
        return Ok(Some((*name).into()));
    }
    let mut remainder = q.to_owned();
    for word in [
        "云效",
        "当前",
        "现在",
        "目前",
        "所有",
        "全部",
        "项目",
        "团队",
        "有哪些",
        "有什么",
        "有啥",
        "哪些",
        "任务",
        "需求",
        "工作项",
        "延期",
        "逾期",
        "负责",
        "帮我",
        "请",
        "查询",
        "查看",
        "查一下",
        "列出",
        "统计",
        "情况",
        "图表",
        "分布",
        "的",
        "有",
    ] {
        remainder = remainder.replace(word, "");
    }
    if remainder.chars().any(char::is_alphanumeric) {
        return Err(RagError::Backend(
            "未在当前云效工作项负责人中识别该姓名，请提供准确姓名；未扩大为全员查询。".into(),
        ));
    }
    Ok(None)
}

async fn overdue_work(
    reader: &SystemQueries,
    context: &QueryContext,
    authorization: &str,
    source: &Value,
) -> Result<GroundedAnswer> {
    if !context.user.can("REQUIREMENT", "READ") {
        return Err(RagError::Backend("当前账号没有读取云效需求工作项的权限。".into()));
    }
    if business::explicit_time(&context.question) {
        return Err(RagError::Backend(
            "当前延期是已同步状态快照，不能还原历史日期的延期情况。请查询当前延期工作项。".into(),
        ));
    }
    let id =
        source["id"].as_str().ok_or_else(|| RagError::Backend("invalid source identity".into()))?;
    let encoded = rag::references::component(id);
    let mut path = format!("/api/devops/sources/{encoded}/records?kind=requirement&delayedOnly=true&page=1&pageSize=12");
    let term =
        context.question.split(['“', '”', '"']).nth(1).filter(|term| !term.trim().is_empty());
    if let Some(term) = term {
        path.push_str(&format!("&search={}", rag::references::component(term)));
    }
    let mut value = reader.get(&format!("{path}&includeOwners=true"), authorization).await?;
    let owner_question =
        term.map_or_else(|| context.question.clone(), |term| context.question.replace(term, ""));
    let owner = overdue_owner(&owner_question, &value["ownerOptions"])?;
    if let Some(name) = &owner {
        value = reader
            .get(&format!("{path}&owner={}", rag::references::component(name)), authorization)
            .await?;
    }
    overdue_answer(context, source, &value, owner.as_deref())
}

fn overdue_answer(
    context: &QueryContext,
    source: &Value,
    value: &Value,
    owner: Option<&str>,
) -> Result<GroundedAnswer> {
    let total = value["total"]
        .as_u64()
        .ok_or_else(|| RagError::Backend("延期工作项统计不完整，不能判断为零。".into()))?;
    let items = value["items"]
        .as_array()
        .ok_or_else(|| RagError::Backend("延期工作项明细不完整。".into()))?;
    if total < items.len() as u64 || (total > 0 && items.is_empty()) {
        return Err(RagError::Backend("延期工作项数量与明细不一致，请重试。".into()));
    }
    if owner.is_some_and(|name| items.iter().any(|row| row["owner"].as_str() != Some(name))) {
        return Err(RagError::Backend(
            "延期工作项负责人范围不一致，请重试；未混入其他人员记录。".into(),
        ));
    }
    let id = source["id"].as_str().unwrap_or_default();
    let url =
        format!("/devops?source={}&view=delivery&delayedOnly=true", rag::references::component(id));
    let mut content = format!("**{}当前延期工作项：{total} 条**\n\n来源：本项目关联的云效需求工作项，按负责人精确筛选；不是 Git 提交或本地拆分任务。\n\n口径与云效统计页面一致：计划截止日期早于今天（上海时间），优先级为紧急／高／中，排除待发布、已发布、已交付、已完成、已关闭、已验收、已拒绝和已取消。\n", owner.unwrap_or("全员"));
    let mut sources =
        vec![json!({"title":"云效延期工作项统计","url":url,"source_type":"business_query"})];
    for row in items.iter().take(12) {
        let serial = row["serial"].as_str().unwrap_or_default();
        let title = row["title"].as_str().unwrap_or("未命名").replace(['[', ']', '\n', '\r'], " ");
        let link = ["url", "webUrl", "sourceUrl"]
            .iter()
            .find_map(|key| row[key].as_str().and_then(rag::references::safe_url))
            .unwrap_or_else(|| {
                format!(
                    "/devops?source={}&view=delivery&reference={}",
                    rag::references::component(id),
                    rag::references::component(row["externalId"].as_str().unwrap_or_default())
                )
            });
        content.push_str(&format!(
            "\n- [{serial} {title}]({link}) · {} · 截止 {} · 负责人 {}",
            row["status"].as_str().unwrap_or("未设置"),
            row["due"].as_str().unwrap_or("未知"),
            row["owner"].as_str().unwrap_or("未归属")
        ));
        sources.push(json!({"title":format!("{serial} {title}"),"url":link,"source_type":"yunxiao_requirement"}));
    }
    if total > items.len() as u64 {
        content.push_str("\n\n仅展示前 12 条，合计包含全部匹配记录。");
    }
    content.push_str(
        "\n\n仅代表最近成功同步的当前快照；未同步、未设置截止日期和历史延期不能按零处理。",
    );
    Ok(GroundedAnswer {
        content,
        sources,
        charts: vec![],
        scope: json!({"kind":"devops_query","projectId":context.project_id,"sourceId":id,"observedAt":source["last_success_at"],"filters":{"overdue":true,"owner":owner},"evidence":{"total":total,"returned":items.len(),"snapshot":true}}),
    })
}

fn prefers_activity(q: &str) -> bool {
    let q = q.to_lowercase();
    ["git", "代码", "提交情况", "提交记录", "摸鱼", "偷懒", "干啥", "干了", "工作情况", "工作内容"]
        .iter()
        .any(|word| q.contains(word))
}
#[cfg(test)]
mod tests {
    #[test]
    fn overdue_questions_use_deadlines_and_exact_known_owners() {
        assert!(super::overdue_work_requested("成员甲有哪些任务延期"));
        assert!(super::overdue_work_requested("云效当前逾期需求有哪些"));
        assert!(!super::overdue_work_requested("研发任务延期有哪些"));
        assert!(!super::overdue_work_requested("任务延期怎么判断"));
        assert!(!super::overdue_work_requested("接口测试任务延期有哪些"));
        let owners = serde_json::json!(["成员甲", "成员乙"]);
        assert_eq!(
            super::overdue_owner("成员甲有哪些任务延期", &owners).unwrap().as_deref(),
            Some("成员甲")
        );
        assert!(super::overdue_owner("成员丙有哪些任务延期", &owners).is_err());
        assert!(super::overdue_owner("成员甲和成员乙有哪些任务延期", &owners).is_err());
        assert_eq!(super::overdue_owner("当前有哪些任务延期", &owners).unwrap(), None);
        assert_eq!(super::overdue_owner("云效延期需求有哪些", &owners).unwrap(), None);
        assert!(super::overdue_owner("我有哪些任务延期", &owners).is_err());
    }
    #[test]
    fn cloud_prefix_does_not_override_git_or_person_activity() {
        assert!(super::prefers_activity("云效和 Git 看成员甲上周的工作情况"));
        assert!(super::prefers_activity("云效本周代码提交有多少"));
        assert!(!super::prefers_activity("云效新增缺陷按提交人统计"));
    }
}
