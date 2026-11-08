use std::sync::Arc;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use rag::{
    domain::{RagError, Result},
    grounding::{GroundedAnswer, QueryContext},
    ports::Chat,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Clone, Copy)]
pub(super) struct Resource {
    pub id: &'static str,
    pub label: &'static str,
    pub permission: &'static str,
    pub route: &'static str,
    from: &'static str,
    project: &'static str,
    visible: &'static str,
    document: &'static str,
}

macro_rules! resource {
    ($id:literal,$label:literal,$permission:literal,$route:literal,$table:literal) => {
        Resource {
            id: $id,
            label: $label,
            permission: $permission,
            route: $route,
            from: concat!($table, " t"),
            project: "t.project_id",
            visible: "true",
            document: "to_jsonb(t)",
        }
    };
}

pub(super) const RESOURCES: &[Resource] = &[
    Resource { visible:"NOT t.deleted", document:"to_jsonb(t) || jsonb_build_object('description',v.description)", from:"ms_requirement t LEFT JOIN ms_requirement_version v ON v.requirement_id=t.id AND v.version=t.baseline_version", ..resource!("requirements","需求","REQUIREMENT","requirement","ms_requirement") },
    Resource { visible:"NOT t.deleted", ..resource!("bugs","缺陷","BUG","bug","ms_bug") },
    Resource { visible:"NOT t.deleted", from:"ms_functional_case t LEFT JOIN ms_test_case_service cs ON cs.case_id=t.id AND cs.project_id=t.project_id LEFT JOIN ms_test_service s ON s.id=cs.service_id AND s.project_id=t.project_id AND NOT s.deleted", document:"to_jsonb(t) || jsonb_build_object('service',s.name)", ..resource!("functional_cases","功能测试用例","FUNCTIONAL_CASE","functional-case","ms_functional_case") },
    Resource { visible:"NOT t.archived", ..resource!("test_plans","测试计划","TEST_PLAN","test-plan","ms_test_plan") },
    Resource { project:"p.project_id", visible:"NOT p.archived", from:"ms_test_plan_run t JOIN ms_test_plan p ON p.id=t.plan_id", document:"to_jsonb(t) || jsonb_build_object('name',p.name)", ..resource!("plan_runs","测试计划执行与报告","TEST_PLAN","test-plan","ms_test_plan_run") },
    Resource { visible:"NOT t.deleted", ..resource!("case_reviews","用例评审","CASE_REVIEW","review","ms_case_review") },
    Resource { visible:"NOT t.deleted", ..resource!("api_definitions","接口定义","API_DEFINITION","api/definition","ms_api_definition") },
    resource!("api_cases","接口测试用例","API_DEFINITION","api/definition","ms_api_case"),
    Resource { visible:"NOT t.deleted", ..resource!("api_scenarios","接口测试场景","API_SCENARIO","api/scenario","ms_api_scenario") },
    Resource { from:"ms_api_case_result t JOIN ms_api_case c ON c.id=t.case_id", project:"c.project_id", document:"to_jsonb(t) || jsonb_build_object('name',c.name)", ..resource!("api_executions","接口用例执行与报告","API_DEFINITION","api/definition","ms_api_case_result") },
    resource!("scenario_executions","接口场景执行与报告","API_SCENARIO","api/scenario","ms_api_scenario_execution"),
    Resource { visible:"NOT COALESCE((to_jsonb(t)->>'archived')::boolean,false)", ..resource!("perf_reports","性能测试报告","PERF","perf","ms_perf_report") },
    Resource { from:"ms_task t JOIN ms_task_decomposition d ON d.id=t.decomposition_id JOIN ms_requirement r ON r.id=d.requirement_id", project:"r.project_id", visible:"NOT r.deleted", ..resource!("tasks","研发任务","TASK","requirement","ms_task") },
    Resource { from:"ms_delivery_attempt t JOIN ms_task_decomposition d ON d.id=t.decomposition_id JOIN ms_requirement r ON r.id=d.requirement_id", project:"r.project_id", visible:"NOT r.deleted", ..resource!("deliveries","交付执行记录","DELIVERY","requirement","ms_delivery_attempt") },
    Resource { from:"ms_design_proposal t JOIN ms_requirement r ON r.id=t.requirement_id", project:"r.project_id", visible:"NOT r.deleted", ..resource!("designs","设计方案","REQUIREMENT","requirement","ms_design_proposal") },
    Resource { from:"ms_verification t JOIN ms_requirement r ON r.id=t.requirement_id", project:"r.project_id", visible:"NOT r.deleted", ..resource!("verifications","需求验收追溯","REQUIREMENT","requirement","ms_verification") },
    Resource { visible:"NOT t.deleted", ..resource!("test_services","项目服务","FUNCTIONAL_CASE","functional-case","ms_test_service") },
    Resource { visible:"NOT t.deleted", ..resource!("functional_scenarios","功能测试场景","FUNCTIONAL_CASE","functional-case","ms_functional_scenario") },
    resource!("functional_runs","功能场景执行记录","FUNCTIONAL_CASE","functional-case","ms_functional_scenario_run"),
    resource!("inventories","服务接口梳理","FUNCTIONAL_CASE","functional-case","ms_test_inventory"),
    resource!("impact_reviews","需求影响面评审","REQUIREMENT","requirement","ms_requirement_impact_review"),
    resource!("files","项目文件元数据","PROJECT","project/files","ms_project_file"),
    Resource { from:"ms_project_member t JOIN ms_user u ON u.id=t.user_id", document:"to_jsonb(t) || jsonb_build_object('id',t.user_id,'name',u.name,'status',t.role)", ..resource!("members","项目成员","PROJECT","project","ms_project_member") },
    Resource { visible:"NOT t.deleted", ..resource!("skills","项目 Skills","SKILL","skill","ms_skill") },
    resource!("templates","项目模板","PROJECT","project/templates","ms_template"),
    Resource { project:"t.id", visible:"NOT t.deleted", ..resource!("project","当前项目","PROJECT","project","ms_project") },
];

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GroupBy {
    #[default]
    Status,
    Priority,
    Module,
    Service,
    Owner,
    Date,
}
impl GroupBy {
    fn sql(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Priority => "priority",
            Self::Module => "module",
            Self::Service => "service",
            Self::Owner => "owner",
            Self::Date => "day",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Status => "状态",
            Self::Priority => "优先级",
            Self::Module => "模块",
            Self::Service => "服务",
            Self::Owner => "负责人／创建人",
            Self::Date => "日期",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub group_by: GroupBy,
    #[serde(default)]
    pub chart: bool,
    #[serde(default)]
    pub updated: bool,
    #[serde(default)]
    pub overdue: bool,
}

fn contains(q: &str, words: &[&str]) -> bool {
    words.iter().any(|w| q.contains(w))
}

pub(super) fn rules(question: &str) -> Option<Plan> {
    let q = question.to_lowercase();
    if contains(
        &q,
        &[
            "怎么实现",
            "如何实现",
            "怎么配置",
            "如何配置",
            "是什么",
            "原理",
            "制度",
            "入职",
            "流程说明",
        ],
    ) {
        return None;
    }
    if !contains(
        &q,
        &[
            "多少",
            "几个",
            "几条",
            "数量",
            "统计",
            "分布",
            "列出",
            "查询",
            "查下",
            "查看",
            "找一下",
            "有哪些",
            "什么情况",
            "进度",
            "概览",
            "汇总",
            "报告",
            "失败",
            "未完成",
            "延期",
            "逾期",
        ],
    ) {
        return None;
    }
    let mut ids = Vec::new();
    let mut add = |id: &str| {
        if !ids.iter().any(|v| v == id) {
            ids.push(id.to_owned());
        }
    };
    if contains(&q, &["全系统", "项目概览", "项目整体", "项目汇总", "项目情况"])
    {
        for id in ["requirements", "bugs", "functional_cases", "test_plans"] {
            add(id);
        }
    }
    if q.contains("影响面") {
        add("impact_reviews");
    } else if contains(&q, &["验收追溯", "验收覆盖"]) {
        add("verifications");
    } else if contains(&q, &["设计方案", "技术方案"]) {
        add("designs");
    } else if q.contains("需求") {
        add("requirements");
    }
    if contains(&q, &["bug", "缺陷"]) {
        add("bugs");
    }
    if q.contains("接口梳理") {
        add("inventories");
    } else if q.contains("接口") || q.contains("api") {
        if contains(&q, &["场景执行", "场景报告"]) {
            add("scenario_executions");
        } else if q.contains("场景") {
            add("api_scenarios");
        } else if contains(&q, &["执行", "报告", "失败"]) {
            add("api_executions");
        } else if q.contains("用例") {
            add("api_cases");
        } else {
            add("api_definitions");
        }
    } else if q.contains("性能") || q.contains("压测") {
        add("perf_reports");
    } else if q.contains("测试计划") && contains(&q, &["报告", "执行记录", "运行记录"])
    {
        add("plan_runs");
    } else if q.contains("测试计划") {
        add("test_plans");
    } else if q.contains("评审") && q.contains("用例") {
        add("case_reviews");
    } else if q.contains("场景") && contains(&q, &["执行", "运行", "报告"]) {
        add("functional_runs");
    } else if q.contains("场景") {
        add("functional_scenarios");
    } else if q.contains("用例") {
        add("functional_cases");
    }
    if q.contains("项目服务") {
        add("test_services");
    }
    if contains(&q, &["研发任务", "开发任务", "任务拆分"]) {
        add("tasks");
    }
    if contains(&q, &["交付执行", "交付记录"]) {
        add("deliveries");
    }
    if contains(&q, &["项目文件", "项目附件"]) {
        add("files");
    }
    if contains(&q, &["项目成员", "项目人员"]) {
        add("members");
    }
    if q.contains("skill") {
        add("skills");
    }
    if q.contains("模板") {
        add("templates");
    }
    if ids.is_empty() {
        return None;
    }
    let group_by = if contains(&q, &["按服务", "各服务", "每个服务"]) {
        GroupBy::Service
    } else if contains(&q, &["按模块", "各模块"]) {
        GroupBy::Module
    } else if contains(&q, &["按人", "负责人分布", "每个人", "各成员"]) {
        GroupBy::Owner
    } else if contains(&q, &["按天", "每天", "趋势", "按日期"]) {
        GroupBy::Date
    } else if contains(&q, &["优先级"]) {
        GroupBy::Priority
    } else {
        GroupBy::Status
    };
    let text = question.split(['“', '”', '"']).nth(1).unwrap_or_default().trim().to_owned();
    let status = if contains(&q, &["失败", "失败的"]) {
        "FAILED"
    } else if contains(&q, &["未完成", "未关闭", "未解决"]) {
        "OPEN"
    } else if q.contains("草稿") {
        "DRAFT"
    } else if contains(&q, &["已交付", "已发布"]) {
        "DELIVERED"
    } else {
        ""
    };
    Some(Plan {
        resources: ids,
        group_by,
        text,
        status: status.into(),
        chart: contains(&q, &["图", "可视化", "分布", "趋势"]),
        updated: contains(&q, &["更新", "修改", "变更"]),
        overdue: contains(&q, &["延期", "逾期"]),
        ..Plan::default()
    })
}

fn literal(question: &str, value: &str) -> bool {
    value.is_empty()
        || (value.chars().count() <= 120 && question.to_lowercase().contains(&value.to_lowercase()))
}
fn validate(plan: &Plan, question: &str) -> Result<()> {
    if plan.resources.is_empty()
        || plan.resources.len() > 4
        || plan.resources.iter().any(|id| !RESOURCES.iter().any(|r| r.id == id))
    {
        return Err(RagError::Backend(
            "查询范围无法确定，请指定需求、缺陷、用例、测试计划或其他业务模块。".into(),
        ));
    }
    if !literal(question, &plan.text) || !(literal(question, &plan.owner) || plan.owner == "我") {
        return Err(RagError::Backend("查询条件未能对应到原问题，请明确名称或负责人。".into()));
    }
    if ![
        "",
        "OPEN",
        "FAILED",
        "DRAFT",
        "DELIVERED",
        "COMPLETED",
        "SUCCESS",
        "PASSED",
        "RUNNING",
        "PENDING",
        "REVIEWED",
    ]
    .contains(&plan.status.as_str())
    {
        return Err(RagError::Backend("无法确定要查询的状态，请明确状态名称。".into()));
    }
    Ok(())
}

pub(super) fn explicit_time(q: &str) -> bool {
    contains(
        q,
        &[
            "今天",
            "昨天",
            "本周",
            "这周",
            "上周",
            "本月",
            "这个月",
            "上月",
            "上个月",
            "近期",
            "最近",
            "今年",
            "去年",
            "季度",
            "半年",
        ],
    ) || regex::Regex::new(r"(?:近|过去)\s*\d+\s*天|\d{4}[-年/]").expect("time pattern").is_match(q)
}

pub(super) struct Business {
    pool: PgPool,
    planner: Arc<dyn Chat>,
}
impl Business {
    pub fn new(pool: PgPool, planner: Arc<dyn Chat>) -> Self {
        Self { pool, planner }
    }
    pub async fn answer(&self, context: &QueryContext) -> Result<Option<GroundedAnswer>> {
        if contains(&context.question, &["可以查询哪些数据", "能查哪些数据", "支持哪些业务数据"])
        {
            let names: Vec<_> = RESOURCES
                .iter()
                .filter(|r| context.user.can(r.permission, "READ"))
                .map(|r| r.label)
                .collect();
            return Ok(Some(GroundedAnswer {content:format!("当前项目可按你的读取权限查询：{}。支持数量、明细、状态分布、时间范围及图表。Git／云效活动使用关联数据源；文档知识使用可见知识库。",names.join("、")),sources:vec![],charts:vec![],scope:json!({"kind":"capabilities","projectId":context.project_id})}));
        }
        if contains(
            &context.question,
            &["入职", "新员工", "新人", "报销", "请假", "考勤", "福利", "办公制度", "账号开通"],
        ) {
            return Ok(None);
        }
        if rag::grounding::needs_change_proof(&context.question) {
            return Ok(None);
        }
        let mut plan = match rules(&context.question) {
            Some(plan) => plan,
            None => {
                let descriptors: Vec<_> =
                    RESOURCES.iter().map(|r| json!({"id":r.id,"label":r.label})).collect();
                let prompt=format!("Classify a request for live project business records. Return JSON only with resources (0-4 IDs from catalog), text (literal quoted name from question, else empty), owner (literal person name from question or 我, else empty), status (empty, OPEN, FAILED, DRAFT, DELIVERED, COMPLETED, SUCCESS, PASSED, RUNNING, PENDING, REVIEWED), group_by (status, priority, module, service, owner, date), chart (boolean), updated (boolean), overdue (boolean). Use resources:[] for policy, how-to, code explanation, document knowledge, Git author/change attribution, or uncertain intent. Never invent names, IDs, filters, SQL, URLs, or dates. Catalog: {}",serde_json::to_string(&descriptors).unwrap_or_default());
                let response = match tokio::time::timeout(
                    std::time::Duration::from_secs(12),
                    self.planner.complete(&prompt, &context.question),
                )
                .await
                {
                    Ok(Ok(value)) => value,
                    _ => return Ok(None),
                };
                let Some(start) = response.find('{') else { return Ok(None) };
                let Some(end) = response.rfind('}') else { return Ok(None) };
                let Ok(plan) = serde_json::from_str::<Plan>(&response[start..=end]) else {
                    return Ok(None);
                };
                if plan.resources.is_empty() {
                    return Ok(None);
                };
                plan
            }
        };
        if plan.owner.is_empty() {
            if contains(&context.question, &["我负责", "我创建", "我的", "我提交"]) {
                plan.owner = "我".into();
            } else if contains(&context.question, &["未分配", "未指派"]) {
                plan.owner =
                    if context.question.contains("未分配") { "未分配" } else { "未指派" }.into();
            } else {
                let names:Vec<String>=sqlx::query_scalar("SELECT DISTINCT u.name FROM ms_project_member m JOIN ms_user u ON u.id=m.user_id WHERE m.project_id=$1")
                    .bind(&context.project_id).fetch_all(&self.pool).await.map_err(storage)?;
                let matched: Vec<_> = names
                    .into_iter()
                    .filter(|name| name.chars().count() > 1 && context.question.contains(name))
                    .collect();
                if matched.len() == 1 {
                    plan.owner = matched[0].clone();
                } else if matched.len() > 1 {
                    return Err(RagError::Backend(
                        "问题包含多位项目成员，请分别查询或使用按人分布。".into(),
                    ));
                }
            }
        }
        if plan.owner.is_empty()
            && contains(&context.question, &["负责的", "创建的", "经办的", "提交的"])
        {
            return Err(RagError::Backend(
                "未识别所指负责人，请提供项目成员姓名或选择按人分布；未扩大到全项目查询。".into(),
            ));
        }
        validate(&plan, &context.question)?;
        self.execute(context, &plan).await.map(Some)
    }
    async fn execute(&self, context: &QueryContext, plan: &Plan) -> Result<GroundedAnswer> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query("SET LOCAL statement_timeout = '5s'")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query("SET LOCAL TIME ZONE 'Asia/Shanghai'")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ms_project_member WHERE project_id=$1 AND user_id=$2)",
        )
        .bind(&context.project_id)
        .bind(&context.user.user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if !member && !context.user.can("SYSTEM_USER", "UPDATE") {
            return Err(RagError::Backend("当前账号不是该项目成员，不能查询项目业务数据。".into()));
        }
        let time = if explicit_time(&context.question) {
            Some(super::activity::window(&context.question, context.at_ms)?)
        } else {
            None
        };
        let from = time.as_ref().map(|w| w.from.to_string()).unwrap_or_default();
        let through = time.as_ref().map(|w| w.through.to_string()).unwrap_or_default();
        let today = Utc
            .timestamp_millis_opt(context.at_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .with_timezone(&FixedOffset::east_opt(28800).expect("Shanghai offset"))
            .date_naive()
            .to_string();
        let mut content = format!(
            "当前项目业务数据 · {}（上海时区）\n",
            if from.is_empty() {
                "截至查询时刻，未限定时间".to_owned()
            } else {
                format!(
                    "{from} 至 {through}，按{}时间筛选",
                    if plan.updated { "更新" } else { "创建／执行" }
                )
            }
        );
        let mut sources = Vec::new();
        let mut charts = Vec::new();
        let mut results = Vec::new();
        for id in &plan.resources {
            let r = RESOURCES.iter().find(|r| r.id == id).expect("validated resource");
            if !context.user.can(r.permission, "READ") {
                content.push_str(&format!("\n{}：无读取权限，未查询，不能判定为零。\n", r.label));
                results.push(json!({"resource":id,"status":"denied"}));
                continue;
            }
            let owner =
                if plan.owner == "我" { context.user.user_id.as_str() } else { &plan.owner };
            let sql = read_sql(r, plan.group_by, plan.updated);
            let value: Value = sqlx::query_scalar(&sql)
                .bind(&context.project_id)
                .bind(&plan.text)
                .bind(owner)
                .bind(&plan.status)
                .bind(&from)
                .bind(&through)
                .bind(plan.overdue)
                .bind(&today)
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
            let total = value["total"].as_i64().ok_or_else(|| {
                RagError::Backend("业务查询返回格式不完整，不能推断统计数量。".into())
            })?;
            let unknown = value["unknownDates"].as_i64().unwrap_or_default();
            let link =
                format!("/p/{}/{}", rag::references::component(&context.project_id), r.route);
            content.push_str(&format!(
                "\n{}：{} 条{}。\n",
                r.label,
                total,
                if !from.is_empty() { "符合时间条件的记录" } else { "" }
            ));
            if !plan.text.is_empty() {
                content.push_str(&format!("名称／描述包含：{}。\n", plan.text));
            }
            if !owner.is_empty() {
                content.push_str(&format!("负责人／创建人：{}。\n", plan.owner));
            }
            if !plan.status.is_empty() {
                content.push_str(&format!("状态条件：{}。\n", plan.status));
            }
            if unknown > 0 && !from.is_empty() {
                content.push_str(&format!(
                    "另有 {unknown} 条缺少对应时间，未纳入日期范围，不能视为该期间零产出。\n"
                ));
            }
            let buckets: Vec<_> = value["buckets"].as_array().into_iter().flatten().collect();
            if !buckets.is_empty() {
                content.push_str(&format!(
                    "按{}：{}。\n",
                    plan.group_by.label(),
                    buckets
                        .iter()
                        .take(12)
                        .map(|v| format!(
                            "{} {}",
                            v["label"].as_str().unwrap_or("未设置"),
                            v["count"]
                        ))
                        .collect::<Vec<_>>()
                        .join("；")
                ));
            }
            for row in value["items"].as_array().into_iter().flatten().take(12) {
                content.push_str(&format!(
                    "- {} · {}{}\n",
                    escape(row["name"].as_str().unwrap_or("未命名")),
                    escape(row["status"].as_str().unwrap_or("状态未设置")),
                    row["date"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(|s| format!(" · {}", format_time(s)))
                        .unwrap_or_default()
                ));
                if !plan.text.is_empty() {
                    if let Some(description) = row["description"].as_str().filter(|s| !s.is_empty())
                    {
                        content.push_str(&format!(
                            "  {}\n",
                            escape(&description.chars().take(700).collect::<String>())
                        ));
                    }
                }
            }
            if total > 12 {
                content.push_str(&format!("明细展示最近 12 条，合计来自全部符合条件的记录；[打开来源页面]({link}) 查看其他记录。\n"));
            }
            let n = sources.len() + 1;
            content.push_str(&format!("[来源 {n}]({link})\n"));
            sources.push(json!({"doc_id":format!("business:{}",r.id),"title":format!("{} · 实时业务查询",r.label),"url":link,"source_type":"business_query","occurred_at":DateTime::<Utc>::from_timestamp_millis(context.at_ms).map(|d|d.to_rfc3339()),"content_preview":format!("{}；{} 条；筛选条件：名称={}；负责人={}；状态={}；{}—{}",r.label,total,plan.text,plan.owner,plan.status,from,through)}));
            if plan.chart {
                charts.push(json!({"kind":"bar","title":format!("{} · 按{}分布",r.label,plan.group_by.label()),"labels":buckets.iter().map(|b|b["label"].clone()).collect::<Vec<_>>(),"values":buckets.iter().map(|b|b["count"].clone()).collect::<Vec<_>>(),"from":from,"through":through,"coverageComplete":unknown==0}));
            }
            results.push(json!({"resource":id,"status":"ok","total":total,"unknownDates":unknown,"buckets":value["buckets"]}));
        }
        tx.commit().await.map_err(storage)?;
        content.push_str("\n以上为当前项目已存储且有权读取的业务记录。外部系统未同步的数据不在此范围；数量不能单独用于评价工作表现。");
        Ok(GroundedAnswer {
            content,
            sources,
            charts,
            scope: json!({"kind":"business_query","projectId":context.project_id,"queriedAt":context.at_ms,"from":from,"through":through,"filters":{"text":plan.text,"owner":plan.owner,"status":plan.status,"overdue":plan.overdue},"results":results}),
        })
    }
}

fn storage(error: sqlx::Error) -> RagError {
    tracing::warn!(error=%error,"business read query failed");
    RagError::Backend(
        "业务数据查询未完成；不能将失败解释为零条记录。请稍后重试或缩小查询范围。".into(),
    )
}
fn escape(s: &str) -> String {
    s.replace(['\n', '\r'], " ").replace(['[', ']', '`'], "")
}
fn format_time(s: &str) -> String {
    DateTime::parse_from_rfc3339(s)
        .map(|d| {
            d.with_timezone(&FixedOffset::east_opt(28800).expect("Shanghai offset"))
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| s.to_owned())
}

fn read_sql(r: &Resource, group: GroupBy, updated: bool) -> String {
    let time = if updated {
        "COALESCE(j->>'updated_at',j->>'modified_at')"
    } else {
        "COALESCE(j->>'executed_at',j->>'triggered_at',j->>'created_at',j->>'added_at')"
    };
    // Identifiers and projections come only from the static catalog; all caller values are bound.
    format!(
        r#"WITH raw AS (
      SELECT {document} AS j FROM {from} WHERE {project}=$1 AND {visible}
    ), projected AS (
      SELECT COALESCE(j->>'id',j->>'report_id',j->>'case_id','') AS id,
        COALESCE(j->>'name',j->>'title',j->>'method',j->>'plan_id',j->>'scenario_id',j->>'requirement_id','未命名') AS name,
        COALESCE((SELECT si.name FROM ms_status_item si WHERE si.project_id=$1 AND si.id=j->>'status'),j->>'status',j->>'outcome',j->>'state','未设置') AS status,
        COALESCE(j->>'priority',j->>'severity','未设置') AS priority,
        COALESCE(j->>'module',j->>'module_id','未设置') AS module,
        COALESCE(j->>'service','未归属') AS service,
        COALESCE(NULLIF(j->>'assignee',''),NULLIF(j->>'handler',''),NULLIF(j->>'executed_by',''),NULLIF(j->>'created_by',''),'') AS owner_id,
        LEFT(COALESCE(j->>'description',j->>'design_doc',''),4000) AS description,
        {time} AS raw_date, COALESCE(j->>'due_date',j->>'end_at','') AS due
      FROM raw
    ), dated AS (
      SELECT projected.*, COALESCE(u.name,NULLIF(owner_id,''),'未设置') AS owner,
        CASE WHEN raw_date ~ '^\d{{13}}$' THEN to_char(to_timestamp(raw_date::double precision/1000),'YYYY-MM-DD"T"HH24:MI:SSOF')
             WHEN raw_date ~ '^\d{{10}}$' THEN to_char(to_timestamp(raw_date::double precision),'YYYY-MM-DD"T"HH24:MI:SSOF')
             WHEN raw_date ~ '^\d{{4}}-\d{{2}}-\d{{2}}' THEN to_char(raw_date::timestamptz,'YYYY-MM-DD"T"HH24:MI:SSOF') ELSE '' END AS date
      FROM projected LEFT JOIN ms_user u ON u.id=owner_id
    ), base AS (
      SELECT *, LEFT(date,10) AS day FROM dated WHERE
       ($2='' OR strpos(lower(name || ' ' || description),lower($2))>0)
       AND ($3='' OR owner_id=$3 OR owner=$3 OR ($3 IN ('未分配','未指派') AND owner_id=''))
       AND ($4='' OR upper(status)=$4 OR ($4='FAILED' AND upper(status) IN ('ERROR','FAIL','FAILED'))
            OR ($4='OPEN' AND upper(status) NOT IN ('CLOSED','COMPLETED','DELIVERED','PASSED','SUCCESS','RESOLVED','DONE','ARCHIVED','已关闭','已解决','已完成')))
       AND (NOT $7 OR (due<>'' AND (CASE WHEN due ~ '^\d{{13}}$' THEN to_char(to_timestamp(due::double precision/1000),'YYYY-MM-DD') ELSE LEFT(due,10) END)<$8 AND upper(status) NOT IN ('CLOSED','COMPLETED','DELIVERED','DONE','ARCHIVED','READY_TO_RELEASE','PENDING_RELEASE','待发布')))
    ), filtered AS (
      SELECT * FROM base WHERE ($5='' OR (date<>'' AND day >= $5 AND day <= $6))
    ), grouped AS (SELECT COALESCE(NULLIF({bucket},''),'未设置') AS label,count(*) AS count FROM filtered GROUP BY 1),
    samples AS (SELECT id,name,status,date,description FROM filtered ORDER BY date DESC,id ASC LIMIT 12)
    SELECT jsonb_build_object('total',(SELECT count(*) FROM filtered),
      'unknownDates',(SELECT count(*) FROM base WHERE date=''),
      'buckets',COALESCE((SELECT jsonb_agg(to_jsonb(x) ORDER BY x.count DESC,x.label) FROM (SELECT * FROM grouped ORDER BY count DESC,label LIMIT 100) x),'[]'::jsonb),
      'items',COALESCE((SELECT jsonb_agg(to_jsonb(samples) ORDER BY date DESC,id ASC) FROM samples),'[]'::jsonb))"#,
        document = r.document,
        from = r.from,
        project = r.project,
        visible = r.visible,
        bucket = group.sql()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_business_queries_use_a_typed_catalog() {
        for (question, id) in [
            ("项目功能用例按服务统计图", "functional_cases"),
            ("本周失败的接口执行报告有哪些", "api_executions"),
            ("延期需求有哪些", "requirements"),
            ("全系统项目概览", "bugs"),
        ] {
            let p = rules(question).expect("business plan");
            assert!(p.resources.iter().any(|v| v == id));
            validate(&p, question).unwrap();
        }
        assert!(rules("接口用例怎么实现").is_none());
    }
    #[test]
    fn planner_cannot_supply_queries_or_invent_filters() {
        assert!(serde_json::from_str::<Plan>(
            r#"{"resources":["bugs"],"sql":"DELETE FROM ms_bug"}"#
        )
        .is_err());
        assert!(validate(&Plan { resources: vec!["secrets".into()], ..Plan::default() }, "统计")
            .is_err());
        assert!(validate(
            &Plan {
                resources: vec!["bugs".into()],
                text: "invented person".into(),
                ..Plan::default()
            },
            "统计缺陷"
        )
        .is_err());
    }
    #[test]
    fn all_resources_have_project_fences_and_parameterized_filters() {
        for r in RESOURCES {
            let sql = read_sql(r, GroupBy::Status, false);
            assert!(sql.contains("=$1 AND"));
            assert!(sql.contains("LIMIT 12"));
            assert!(!sql.contains("content_b64"));
            assert!(!sql.contains("instructions"));
        }
    }
    struct NoModel;
    #[async_trait::async_trait]
    impl Chat for NoModel {
        async fn complete(&self, _system: &str, _user: &str) -> Result<String> {
            panic!("facts must not call generation")
        }
    }
    #[tokio::test]
    #[ignore = "requires an isolated SHEPHERD_QA_TEST_DATABASE_URL"]
    async fn catalog_database_contract_and_scope() {
        let url = std::env::var("SHEPHERD_QA_TEST_DATABASE_URL").expect("isolated database");
        let pool = PgPool::connect(&url).await.unwrap();
        migrate::run(&pool).await.unwrap();
        let project = uuid::Uuid::new_v4().to_string();
        let user = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO ms_project(id,organization_id,name) VALUES ($1,'test',$1)")
            .bind(&project)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ms_user(id,name,email) VALUES ($1,'Member A',$2)")
            .bind(&user)
            .bind(format!("{user}@example.invalid"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ms_project_member(project_id,user_id) VALUES ($1,$2)")
            .bind(&project)
            .bind(&user)
            .execute(&pool)
            .await
            .unwrap();
        let permissions = webauth::PermissionSet::from_raw(
            RESOURCES.iter().map(|r| format!("{}:READ", r.permission)),
        )
        .unwrap();
        let mut context = QueryContext {
            user: webauth::AuthUser { user_id: user, permissions },
            project_id: project.clone(),
            question: "功能用例统计图".into(),
            history: vec![],
            at_ms: 1794146400000,
        };
        let business = Business::new(pool.clone(), Arc::new(NoModel));
        for r in RESOURCES {
            let result = business
                .execute(&context, &Plan { resources: vec![r.id.into()], ..Plan::default() })
                .await;
            assert!(result.is_ok(), "{}: {:?}", r.id, result.err());
        }
        for (name, scope, deleted) in [
            ("visible", project.as_str(), false),
            ("foreign", "another-project", false),
            ("deleted", project.as_str(), true),
        ] {
            sqlx::query("INSERT INTO ms_functional_case(project_id,name,deleted,created_at) VALUES ($1,$2,$3,'2026-11-04T16:30:00Z')").bind(scope).bind(name).bind(deleted).execute(&pool).await.unwrap();
        }
        let plan =
            Plan { resources: vec!["functional_cases".into()], chart: true, ..Plan::default() };
        let result = business.execute(&context, &plan).await.unwrap();
        assert_eq!(result.scope["results"][0]["total"], 1);
        assert!(!result.content.contains("foreign"));
        assert!(!result.content.contains("deleted"));
        assert_eq!(result.charts[0]["values"][0], 1);
        context.question = "2026-11-05 功能用例统计".into();
        assert_eq!(
            business.execute(&context, &plan).await.unwrap().scope["results"][0]["total"],
            1
        );
        context.question = "2026-11-04 功能用例统计".into();
        assert_eq!(
            business.execute(&context, &plan).await.unwrap().scope["results"][0]["total"],
            0
        );
        context.user.permissions = webauth::PermissionSet::from_raw(["PROJECT:READ"]).unwrap();
        let result = business.execute(&context, &plan).await.unwrap();
        assert_eq!(result.scope["results"][0]["status"], "denied");
        assert!(result.sources.is_empty());
        context.user.user_id = "outsider".into();
        assert!(business.execute(&context, &plan).await.is_err());
    }
}
