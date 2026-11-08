use super::{activity, business, SystemQueries};
use rag::{
    domain::{RagError, Result},
    grounding::{GroundedAnswer, QueryContext},
};
use serde_json::json;

pub(super) async fn answer(
    reader: &SystemQueries,
    context: &QueryContext,
    authorization: &str,
) -> Result<Option<GroundedAnswer>> {
    let q = &context.question;
    if !q.contains("云效")
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
            let row = &item["data"];
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
