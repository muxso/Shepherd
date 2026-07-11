use std::error::Error;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

type R<T> = Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(name = "shepherd", version, about = "Shepherd —— AI 研发监督平台 CLI")]
struct Cli {
    /// 以原始 JSON 输出(默认渲染人类可读表格 / 键值)。便于脚本/管道消费。
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

static JSON_OUTPUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Subcommand)]
enum Cmd {
    /// 登录并保存会话。
    Login {
        #[arg(long, default_value = "http://localhost:8088")]
        url: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        password: String,
    },
    /// 生成上手脚手架(需求模板 + 快速上手),离线、不联网。
    Init {
        #[arg(long, default_value = ".")]
        dir: String,
        #[arg(long)]
        force: bool,
    },
    /// 需求管理。
    Req {
        #[command(subcommand)]
        cmd: ReqCmd,
    },
    /// 为需求版本开启任务拆分图。
    Decompose {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
    },
    /// 复查任务拆分图(按 id 读回完整 DAG + 各任务当前状态)。
    Decomposition {
        #[command(subcommand)]
        cmd: DecompositionCmd,
    },
    /// 任务管理。
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// 把任务派发给 AI 执行者(对应 README 的 `task run`)。
    Dispatch {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        title: String,
        /// 执行者(默认用 `agent connect` 连接的;未连接则 CLAUDE_CODE)。
        #[arg(long)]
        executor: Option<String>,
        #[arg(long)]
        instructions: Option<String>,
        /// 可选:项目 id(配合 --skills 自动组合行为规范)。
        #[arg(long)]
        project: Option<String>,
        /// 可选:skill id 逗号分隔,派发前自动 compose 为行为规范注入。
        #[arg(long, value_delimiter = ',')]
        skills: Vec<String>,
    },
    /// 完整性验证。
    Verify {
        #[command(subcommand)]
        cmd: VerifyCmd,
    },
    /// AI Skill 编排。
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// 连接/查看 AI 执行者(dispatch 的默认 executor)。
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// 注销当前会话(撤销服务端令牌并清空本地 token)。
    Logout,
    /// 项目管理。
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },
    /// 缺陷管理。
    Bug {
        #[command(subcommand)]
        cmd: BugCmd,
    },
    /// 用例评审。
    Case {
        #[command(subcommand)]
        cmd: CaseCmd,
    },
    /// 测试计划。
    Plan {
        #[command(subcommand)]
        cmd: PlanCmd,
    },
    /// 接口测试(批量执行)。
    Api {
        #[command(subcommand)]
        cmd: ApiCmd,
    },
    /// 用户管理。
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
    /// 组织管理。
    Org {
        #[command(subcommand)]
        cmd: OrgCmd,
    },
    /// 角色与授权。
    Role {
        #[command(subcommand)]
        cmd: RoleCmd,
    },
    /// 接口定义(目录 + 用例 + Mock)。
    Apidef {
        #[command(subcommand)]
        cmd: ApidefCmd,
    },
    /// 场景(编排 + 步骤 + 编译 + 运行)。
    Scenario {
        #[command(subcommand)]
        cmd: ScenarioCmd,
    },
    /// 环境(项目级 base_url + 默认头 + 变量;运行时注入)。
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
    },
    /// 功能用例(CRUD + 自定义字段 + Excel 导出)。
    Fcase {
        #[command(subcommand)]
        cmd: FcaseCmd,
    },
    /// 远程执行 agent(按环境注册 + 把用例派给 agent 就地执行)。
    Runner {
        #[command(subcommand)]
        cmd: RunnerCmd,
    },
    /// 原生压测(并发施压 + 延迟分位/吞吐报告;无 JMeter)。
    Perf {
        #[command(subcommand)]
        cmd: PerfCmd,
    },
    /// 资源池(批量/场景执行的执行节点归属;batch-run/scenario run 需要 --pool)。
    Pool {
        #[command(subcommand)]
        cmd: PoolCmd,
    },
}

#[derive(Subcommand)]
enum PerfCmd {
    /// 发起一轮压测(后台执行,立即返回 reportId)。
    Run {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
        /// 并发"虚拟用户"数。
        #[arg(long, default_value_t = 10)]
        concurrency: u32,
        /// 合计请求次数(时长模式下被忽略)。
        #[arg(long, default_value_t = 100)]
        iterations: u32,
        /// 时长模式:持续压测该毫秒数(给定则忽略 --iterations)。
        #[arg(long = "duration-ms")]
        duration_ms: Option<u64>,
        /// 断言:期望状态码(HTTP=状态码;其它协议 OK 时为 0)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
        /// 断言:输出包含子串。
        #[arg(long)]
        contains: Option<String>,
        /// 断言:输出等于。
        #[arg(long)]
        equals: Option<String>,
        /// 断言:单次延迟不超过该毫秒数。
        #[arg(long = "latency-under")]
        latency_under: Option<u64>,
        /// 协议:HTTP(默认)| SQL | GRPC | REDIS | MYSQL | WEBSOCKET。非 HTTP 时 --url 为目标、--query 为载荷。
        #[arg(long, default_value = "HTTP")]
        protocol: String,
        /// 协议载荷:SQL=语句、GRPC=方法路径、REDIS=命令、WS=消息。
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value = "")]
        project: String,
    },
    /// 查压测报告(吞吐/错误率/延迟分位)。
    Report {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum PoolCmd {
    /// 新建资源池。
    Create {
        #[arg(long)]
        name: String,
        /// 标记禁用(默认启用)。
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 列出资源池。
    List,
}

#[derive(Subcommand)]
enum AgentCmd {
    /// 连接一个 AI 执行者(claude-code | codex | opencode | codebuddy),保存为 dispatch 默认。
    Connect {
        #[arg(long = "type")]
        kind: String,
    },
    /// 查看当前连接 / 登录 / 服务状态。
    Status,
    /// 断开(dispatch 回落默认 CLAUDE_CODE)。
    Disconnect,
}

fn normalize_agent(t: &str) -> R<String> {
    match t.to_ascii_lowercase().replace('-', "_").as_str() {
        "claude_code" => Ok("CLAUDE_CODE".into()),
        "codex" => Ok("CODEX".into()),
        "opencode" => Ok("OPENCODE".into()),
        // 品牌是一个词,但 claude-code 的写法会诱导 code-buddy,一并接受。
        "codebuddy" | "code_buddy" => Ok("CODEBUDDY".into()),
        other => Err(format!(
            "未知 agent 类型: {other}(支持 claude-code | codex | opencode | codebuddy)"
        )
        .into()),
    }
}

#[derive(Subcommand)]
enum ReqCmd {
    /// 新建需求。
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        /// 验收标准,逗号分隔。
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// 列出项目内需求。
    List {
        #[arg(long)]
        project: String,
        /// 页码(从 1 起)。
        #[arg(long, default_value_t = 1)]
        page: u32,
        /// 每页条数。
        #[arg(long, default_value_t = 50)]
        page_size: u32,
    },
    /// 自动拆分需求为任务 DAG(服务端取规格,交规划器拆分)。
    Breakdown {
        #[arg(long)]
        req: String,
        /// 可选:指定需求版本(默认基线版本)。
        #[arg(long)]
        version: Option<u32>,
        /// 使用 AI 规划器(默认即用服务端配置的规划器;此标志仅为可读性)。
        #[arg(long, default_value_t = false)]
        ai: bool,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// 向拆分图加任务。
    Add {
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        title: String,
        /// 依赖任务本地 id,逗号分隔。
        #[arg(long, value_delimiter = ',')]
        deps: Vec<String>,
    },
}

#[derive(Subcommand)]
enum DecompositionCmd {
    /// 读回完整拆分图(含 complete / readyTaskIds / 各任务当前状态)。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 只看当前就绪(依赖全部 Verified、可派发)的任务。
    Ready {
        #[arg(long)]
        id: String,
    },
    /// 并行编排:按依赖图逐层并发派发整张任务 DAG(自动驱动验证门)。
    Run {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "CLAUDE_CODE")]
        executor: String,
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
}

#[derive(Subcommand)]
enum VerifyCmd {
    /// 开启验证(传入验收标准)。
    Create {
        #[arg(long)]
        req: String,
        #[arg(long, default_value_t = 1)]
        version: u32,
        #[arg(long, value_delimiter = ',')]
        criteria: Vec<String>,
    },
    /// 建立覆盖链:某任务覆盖某条验收标准(需求 → 任务 追溯)。
    Link {
        /// 验证 id。
        #[arg(long)]
        id: String,
        /// 验收标准下标(0 起)。
        #[arg(long)]
        criterion: u32,
        /// 拆分图 id。
        #[arg(long)]
        decomp: String,
        /// 任务本地 id(如 t1)。
        #[arg(long)]
        task: String,
    },
    /// 同步任务的交付/验证状态到其覆盖链(任务 → 实现 追溯)。
    Sync {
        #[arg(long)]
        id: String,
        #[arg(long)]
        decomp: String,
        #[arg(long)]
        task: String,
        /// 标记为未验证(默认按已验证 satisfied=true 同步)。
        #[arg(long, default_value_t = false)]
        unsatisfied: bool,
    },
    /// 完整性报告。
    Report {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// 定义 skill。
    Add {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        instructions: String,
        #[arg(long, value_delimiter = ',')]
        includes: Vec<String>,
    },
    /// 组合 skill → 指令集。
    Compose {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// 新建项目。
    Create {
        #[arg(long)]
        org: String,
        #[arg(long)]
        name: String,
    },
    /// 分页列出组织内项目。
    List {
        #[arg(long)]
        org: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
}

#[derive(Subcommand)]
enum BugCmd {
    /// 新建缺陷。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "NEW")]
        status: String,
    },
    /// 流转缺陷状态。
    Status {
        #[arg(long)]
        id: String,
        #[arg(long)]
        to: String,
    },
}

#[derive(Subcommand)]
enum CaseCmd {
    /// 提交一次用例评审意见。
    Review {
        #[arg(long)]
        review: String,
        #[arg(long)]
        case: String,
        #[arg(long)]
        reviewer: String,
        /// PASS | UN_PASS | UNDER_REVIEWED。
        #[arg(long)]
        status: String,
        /// UN_PASS 必填。
        #[arg(long)]
        content: Option<String>,
    },
}

#[derive(Subcommand)]
enum RunnerCmd {
    /// 注册一个环境内的 runner-agent。
    Register {
        #[arg(long)]
        name: String,
        /// agent 入口,如 http://10.0.0.5:9100。
        #[arg(long = "base-url")]
        base_url: String,
        /// 可选共享密钥(对应 agent 的 RUNNER_TOKEN)。
        #[arg(long)]
        token: Option<String>,
    },
    /// 列出已注册 agent。
    List,
    /// 把一条自包含用例派给某 agent 就地执行。
    Run {
        /// agent id(来自 register/list)。
        #[arg(long)]
        agent: String,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        body: Option<String>,
        /// 期望状态码(给定则生成 StatusIs 断言)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// 把一条**已存储的用例**(api-case)派给某 agent 执行。
    RunCase {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        case: String,
    },
    /// 查某 agent 的远程执行历史。
    Executions {
        #[arg(long)]
        agent: String,
    },
    /// 重拉某 agent 的 /protocols,刷新其协议能力快照。
    Refresh {
        #[arg(long)]
        agent: String,
    },
    /// 按协议探测:只给协议,中央自动选支持它的 agent 就地执行(支持断言)。
    Probe {
        /// 协议名(http/grpc/sql/…)。
        #[arg(long)]
        protocol: String,
        /// 目标(URL / gRPC 端点 / 连接串)。
        #[arg(long)]
        target: String,
        /// 载荷(HTTP body / gRPC 请求字节 / SQL 语句)。
        #[arg(long)]
        payload: Option<String>,
        /// 协议附加参数 k=v(可重复;如 method=POST、gRPC 的 method=/pkg.Svc/M)。
        #[arg(long = "meta")]
        meta: Vec<String>,
        /// 断言:状态码等于。
        #[arg(long = "expect-status")]
        expect_status: Option<i64>,
        /// 断言:输出包含子串。
        #[arg(long)]
        contains: Option<String>,
        /// 断言:延迟不超过 ms。
        #[arg(long = "latency-under")]
        latency_under: Option<u64>,
    },
}

#[derive(Subcommand)]
enum FcaseCmd {
    /// 新建功能用例。--field key=value(可重复)设自定义字段。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        module: String,
        #[arg(long, default_value = "P2")]
        priority: String,
        #[arg(long, default_value = "PREPARED")]
        status: String,
        #[arg(long = "field")]
        fields: Vec<String>,
    },
    /// 列出项目内功能用例。
    List {
        #[arg(long)]
        project: String,
    },
    /// 导出为 Excel(.xlsx)写入 --out 文件。
    Export {
        #[arg(long)]
        project: String,
        #[arg(long, default_value = "cases.xlsx")]
        out: String,
    },
    /// 从 Excel(.xlsx)导入用例。
    Import {
        #[arg(long)]
        project: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum PlanCmd {
    /// 新建测试计划(或分组)。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// TEST_PLAN | GROUP。
        #[arg(long = "type", default_value = "TEST_PLAN")]
        plan_type: String,
        #[arg(long)]
        group: Option<String>,
    },
    /// 计划执行统计。
    Stats {
        #[arg(long)]
        id: String,
    },
    /// 导出报告到 stdout:默认 HTML(可 `> report.html`);--markdown 导出 Markdown(可 `> report.md`)。
    Report {
        #[arg(long)]
        id: String,
        /// 导出 Markdown(而非 HTML)。
        #[arg(long)]
        markdown: bool,
    },
    /// 给计划配定时执行(cron 6 段:秒 分 时 日 月 周)。
    Schedule {
        #[arg(long)]
        id: String,
        /// cron 表达式,如 "0 */5 * * * *"(每 5 分钟)。
        #[arg(long)]
        cron: String,
    },
    /// 查计划的定时运行快照(通过率/执行率趋势)。
    Runs {
        #[arg(long)]
        id: String,
    },
    /// 把一条用例挂入计划。
    LinkCase {
        #[arg(long)]
        id: String,
        #[arg(long)]
        case: String,
        #[arg(long, default_value = "")]
        name: String,
    },
    /// 回写某用例在计划内的执行结果。
    Result {
        #[arg(long)]
        id: String,
        #[arg(long)]
        case: String,
        /// SUCCESS | ERROR | FAKE_ERROR | BLOCK | PENDING。
        #[arg(long, default_value = "SUCCESS")]
        status: String,
        #[arg(long = "latency-ms", default_value_t = 0)]
        latency_ms: u64,
        #[arg(long = "status-code")]
        status_code: Option<i64>,
        #[arg(long)]
        body: Option<String>,
        /// 断言数组 JSON,如 '[{"item":"状态码","actual":"200","condition":"等于","expected":"200","passed":true}]'。
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
    },
    /// 列出计划内用例(含状态)。
    Cases {
        #[arg(long)]
        id: String,
    },
    /// 执行计划:跑挂入的用例并自动回写结果(随后 `plan report` 即真实数据)。
    Run {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum ApiCmd {
    /// 批量执行接口用例。
    BatchRun {
        #[arg(long)]
        project: String,
        #[arg(long, value_delimiter = ',')]
        cases: Vec<String>,
        /// 运行模式(如 SERIAL | PARALLEL)。
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        /// 资源池 id(批量执行需客户端提供)。
        #[arg(long)]
        pool: Option<String>,
        /// 运行所用环境 id(注入 base_url/默认头/变量)。
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
enum UserCmd {
    /// 新建用户。
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
    },
    /// 分页列出用户。
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 查看单个用户。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新用户。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: String,
        /// 标记禁用(默认启用)。
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 删除用户。
    Delete {
        #[arg(long)]
        id: String,
    },
    /// 按 id 批量解析用户名(逗号分隔)。
    Names {
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum OrgCmd {
    /// 新建组织。
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 分页列出组织。
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 查看单个组织。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新组织。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        disable: bool,
    },
    /// 删除组织。
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum RoleCmd {
    /// 新建角色。
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        /// 权限串,逗号分隔(如 PROJECT:READ+ADD)。
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// 分页列出角色。
    List {
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 查看单个角色。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新角色。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, value_delimiter = ',')]
        permissions: Vec<String>,
    },
    /// 删除角色。
    Delete {
        #[arg(long)]
        id: String,
    },
    /// 给用户授予角色。
    Grant {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
    /// 撤销用户的角色。
    Revoke {
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
    },
}

#[derive(Subcommand)]
enum ApidefCmd {
    /// 新建接口定义。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// 接口类型 HTTP | TCP | SQL | DUBBO。
        #[arg(long, default_value = "HTTP")]
        protocol: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long, default_value = "")]
        path: String,
    },
    /// 从 OpenAPI 3.x / Swagger 2.0 批量导入接口定义(--file 本地 / --url 远程二选一)。
    Import {
        #[arg(long)]
        project: String,
        /// OpenAPI/Swagger JSON 文件路径。
        #[arg(long)]
        file: Option<String>,
        /// OpenAPI/Swagger JSON 的 URL(如服务自身的 /api-docs/openapi.json)。
        #[arg(long)]
        url: Option<String>,
    },
    /// 列出项目内接口定义。
    List {
        #[arg(long)]
        project: String,
    },
    /// 查看单个接口定义。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 给定义加接口用例(落 ms_api_case,可被批量运行)。
    Case {
        #[arg(long)]
        def: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        body: Option<String>,
        /// 期望状态码:给定即生成 StatusIs 断言(决定用例成功/失败);省略则空断言(恒成功)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// 列出定义下的接口用例。
    Cases {
        #[arg(long)]
        def: String,
    },
    /// 新建接口用例(可独立于定义:省略 --def 即独立用例)。
    CaseNew {
        #[arg(long)]
        project: String,
        #[arg(long)]
        def: Option<String>,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "GET")]
        method: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        body: Option<String>,
        /// 期望状态码:给定即生成 StatusIs 断言(决定用例成功/失败);省略则空断言(恒成功)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
    },
    /// 为项目内每个接口定义批量生成「成功(期望 2xx)+ 失败(期望 401)」用例,并各建一条场景串联两者。
    GenSuite {
        #[arg(long)]
        project: String,
        /// 用例请求的基础 URL(拼到 OpenAPI path 前)。
        #[arg(long, default_value = "http://localhost:8088")]
        base: String,
        /// 仅生成用例,不建场景。
        #[arg(long = "no-scenario", default_value_t = false)]
        no_scenario: bool,
    },
    /// 分页列出项目内接口用例(独立视图)。
    CaseList {
        #[arg(long)]
        project: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 分页查看某用例的执行记录。
    CaseExec {
        #[arg(long)]
        case: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 单独执行某条接口用例(可选环境/资源池),回写执行记录。
    CaseRun {
        #[arg(long)]
        case: String,
        #[arg(long)]
        project: String,
        #[arg(long = "mode", default_value = "SERIAL")]
        run_mode: String,
        #[arg(long)]
        pool: Option<String>,
        /// 运行所用环境 id(注入 base_url/默认头/变量)。
        #[arg(long)]
        env: Option<String>,
    },
    /// 给定义加 Mock。
    Mock {
        #[arg(long)]
        def: String,
        #[arg(long)]
        name: String,
        #[arg(long = "status", default_value_t = 200)]
        response_status: i32,
        #[arg(long)]
        body: Option<String>,
    },
    /// 列出定义下的 Mock。
    Mocks {
        #[arg(long)]
        def: String,
    },
}

#[derive(Subcommand)]
enum ScenarioCmd {
    /// 新建场景。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
    },
    /// 列出项目内场景。
    List {
        #[arg(long)]
        project: String,
    },
    /// 查看单个场景(含步骤)。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 加步骤:kind=request|case|scenario|loop|if|once|timer。
    Step {
        #[arg(long)]
        scenario: String,
        /// request | case | scenario | loop | if | once | timer。
        #[arg(long)]
        kind: String,
        #[arg(long = "ref-mode", default_value = "REFERENCE")]
        ref_mode: String,
        #[arg(long, default_value_t = 1)]
        order: i32,
        /// case/scenario 步骤:被引用的 id。
        #[arg(long = "ref")]
        ref_id: Option<String>,
        /// request 步骤:内联请求。
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        body: Option<String>,
        /// request 步骤:期望状态码(生成 StatusIs 断言)。
        #[arg(long = "expect-status")]
        expect_status: Option<u16>,
        /// request 步骤:断言数组 JSON(覆盖 --expect-status),如
        /// '[{"type":"StatusIs","args":200},{"type":"BodyContains","args":"ok"}]'。
        #[arg(long = "assertions-json")]
        assertions_json: Option<String>,
        /// 控制器步骤(loop/if/once/timer)的载荷 JSON,如
        /// '{"times":3,"children":[{"kind":"CASE","refId":"c1"}]}'。
        #[arg(long = "control-json")]
        control_json: Option<String>,
    },
    /// 编译场景为可运行步骤(递归展开子场景)。
    Compile {
        #[arg(long)]
        id: String,
    },
    /// 分页查看场景执行记录(含执行状态)。
    Executions {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 1)]
        current: u32,
        #[arg(long = "page-size", default_value_t = 10)]
        page_size: u32,
    },
    /// 运行场景(编译 → 批量执行)。
    Run {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long = "mode", default_value = "PARALLEL")]
        run_mode: String,
        #[arg(long)]
        pool: Option<String>,
        /// 运行所用环境 id(注入 base_url/默认头/变量)。
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
enum EnvCmd {
    /// 新建环境。--header 形如 "Name: value"(可重复);--var 形如 key=value(可重复)。
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        /// 基础地址(相对 url 前缀),如 http://localhost:8088。
        #[arg(long, default_value = "")]
        base: String,
        #[arg(long = "header")]
        headers: Vec<String>,
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// 列出项目内环境。
    List {
        #[arg(long)]
        project: String,
    },
    /// 查看单个环境。
    Get {
        #[arg(long)]
        id: String,
    },
    /// 更新环境(整体覆盖 name/base/headers/vars)。
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        base: String,
        #[arg(long = "header")]
        headers: Vec<String>,
        #[arg(long = "var")]
        vars: Vec<String>,
    },
    /// 删除环境。
    Delete {
        #[arg(long)]
        id: String,
    },
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Config {
    url: String,
    token: String,
    #[serde(default)]
    agent: Option<String>,
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".shepherd").join("config.json")
}

impl Config {
    fn load() -> Config {
        let mut c: Config = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if let Ok(u) = std::env::var("SHEPHERD_URL") {
            c.url = u;
        }
        if let Ok(t) = std::env::var("SHEPHERD_TOKEN") {
            c.token = t;
        }
        if c.url.is_empty() {
            c.url = "http://127.0.0.1:8088".into();
        }
        c
    }

    fn save(&self) -> R<()> {
        let p = config_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

struct Client {
    http: reqwest::blocking::Client,
    cfg: Config,
}

impl Client {
    fn new(cfg: Config) -> R<Client> {
        // no_proxy:服务通常本地/内网,避免被全局代理劫持。
        let http = reqwest::blocking::Client::builder().no_proxy().build()?;
        Ok(Client { http, cfg })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.url.trim_end_matches('/'), path)
    }

    fn send(&self, mut rb: reqwest::blocking::RequestBuilder, auth: bool) -> R<Value> {
        if auth {
            if self.cfg.token.is_empty() {
                return Err("未登录:先执行 `shepherd login`".into());
            }
            rb = rb.bearer_auth(&self.cfg.token);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn post(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.post(self.url(path)).json(&body), auth)
    }
    fn get(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.get(self.url(path)), auth)
    }
    fn post_bytes(&self, path: &str, bytes: Vec<u8>, auth: bool) -> R<Value> {
        self.send(
            self.http
                .post(self.url(path))
                .header("content-type", "application/octet-stream")
                .body(bytes),
            auth,
        )
    }
    fn get_bytes(&self, path: &str, auth: bool) -> R<Vec<u8>> {
        let mut rb = self.http.get(self.url(path));
        if auth {
            if self.cfg.token.is_empty() {
                return Err("未登录:先执行 `shepherd login`".into());
            }
            rb = rb.bearer_auth(&self.cfg.token);
        }
        let resp = rb.send()?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", resp.text().unwrap_or_default()).into());
        }
        Ok(resp.bytes()?.to_vec())
    }
    fn get_text(&self, path: &str, auth: bool) -> R<String> {
        let mut rb = self.http.get(self.url(path));
        if auth {
            if self.cfg.token.is_empty() {
                return Err("未登录:先执行 `shepherd login`".into());
            }
            rb = rb.bearer_auth(&self.cfg.token);
        }
        let resp = rb.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("HTTP {status}: {text}").into());
        }
        Ok(text)
    }
    fn put(&self, path: &str, body: Value, auth: bool) -> R<Value> {
        self.send(self.http.put(self.url(path)).json(&body), auth)
    }
    fn fetch_text(&self, url: &str) -> R<String> {
        let resp = self.http.get(url).send()?;
        if !resp.status().is_success() {
            return Err(format!("拉取 {url} 失败:HTTP {}", resp.status()).into());
        }
        Ok(resp.text()?)
    }
    fn delete(&self, path: &str, auth: bool) -> R<Value> {
        self.send(self.http.delete(self.url(path)), auth)
    }
}

fn pretty(v: &Value) {
    if JSON_OUTPUT.load(std::sync::atomic::Ordering::Relaxed) {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
        return;
    }
    render_human(v);
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => "—".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => format!("[{} 项]", a.len()),
        Value::Object(o) => format!("{{{} 字段}}", o.len()),
    }
}

fn render_human(v: &Value) {
    if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
        render_table(items);
        let total = v.get("total").map(cell).unwrap_or_default();
        let cur = v.get("current").map(cell).unwrap_or_default();
        let pages = v.get("totalPages").map(cell).unwrap_or_default();
        if !total.is_empty() {
            println!("\n共 {total} 条 · 第 {cur} 页 / 共 {pages} 页");
        }
        return;
    }
    match v {
        Value::Array(a) => render_table(a),
        Value::Object(_) => render_kv(v),
        other => println!("{}", cell(other)),
    }
}

fn render_table(items: &[Value]) {
    if items.is_empty() {
        println!("(空)");
        return;
    }
    let mut cols: Vec<String> = Vec::new();
    for it in items {
        if let Some(o) = it.as_object() {
            for k in o.keys() {
                if !cols.contains(k) {
                    cols.push(k.clone());
                }
            }
        }
    }
    if cols.is_empty() {
        for it in items {
            println!("{}", cell(it));
        }
        return;
    }
    let trunc = |s: String| {
        if s.chars().count() > 40 {
            format!("{}…", s.chars().take(39).collect::<String>())
        } else {
            s
        }
    };
    let mut widths: Vec<usize> = cols.iter().map(|c| c.chars().count()).collect();
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|it| {
            cols.iter()
                .map(|c| trunc(it.get(c).map(cell).unwrap_or_else(|| "—".to_string())))
                .collect()
        })
        .collect();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let pad = |s: &str, w: usize| {
        let n = s.chars().count();
        format!("{}{}", s, " ".repeat(w.saturating_sub(n)))
    };
    let header: Vec<String> = cols.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
    println!("{}", header.join("  "));
    println!("{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  "));
    for r in &rows {
        let line: Vec<String> = r.iter().enumerate().map(|(i, c)| pad(c, widths[i])).collect();
        println!("{}", line.join("  "));
    }
}

fn render_kv(v: &Value) {
    if let Some(o) = v.as_object() {
        let w = o.keys().map(|k| k.chars().count()).max().unwrap_or(0);
        for (k, val) in o {
            let pad = " ".repeat(w.saturating_sub(k.chars().count()));
            println!("{k}{pad} : {}", cell(val));
        }
    }
}

fn status_assertions(expect_status: Option<u16>) -> Value {
    match expect_status {
        Some(code) => json!([{ "type": "StatusIs", "args": code }]),
        None => json!([]),
    }
}

fn parse_headers(items: &[String]) -> R<Value> {
    let mut arr = Vec::with_capacity(items.len());
    for it in items {
        let (name, value) =
            it.split_once(':').ok_or_else(|| format!("--header 需 'Name: value' 格式:{it}"))?;
        arr.push(json!({"name": name.trim(), "value": value.trim()}));
    }
    Ok(Value::Array(arr))
}

fn parse_vars(items: &[String]) -> R<Value> {
    let mut map = serde_json::Map::with_capacity(items.len());
    for it in items {
        let (k, v) = it.split_once('=').ok_or_else(|| format!("--var 需 'key=value' 格式:{it}"))?;
        map.insert(k.trim().to_string(), json!(v));
    }
    Ok(Value::Object(map))
}

const TPL_REQUIREMENT: &str = "# 需求模板

标题: 用户登录
描述: 支持邮箱 + 密码登录,签发会话令牌。

验收标准:
- 正确凭证返回 token
- 错误凭证返回 401
- 令牌过期后需重新登录

改好后录入(需先 `shepherd login`):

    shepherd req add --project <projectId> \\
      --title \"用户登录\" \\
      --description \"支持邮箱 + 密码登录,签发会话令牌。\" \\
      --criteria \"正确凭证返回 token,错误凭证返回 401,令牌过期后需重新登录\"
";

const TPL_GETTING_STARTED: &str = "# Shepherd 上手

前置:一个运行中的 server(默认 http://localhost:8088)。

1. 登录:`shepherd login --url http://localhost:8088 --user admin --password <pw>`
2. 录入需求:见 `requirements/example.md`
3. 拆分任务:`shepherd decompose --req <requirementId> --version 1`
4. 派发执行:`shepherd dispatch --decomp <decompositionId> --task <taskId> --executor CLAUDE_CODE`
5. 验证 / 复查:`shepherd verify --help`、`shepherd decomposition --help`

各命令的完整参数见 `shepherd <命令> --help`。
";

/// 脚手架文件清单(相对路径 → 内容)。纯函数,便于测试。
fn scaffold_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("requirements/example.md", TPL_REQUIREMENT),
        ("shepherd.getting-started.md", TPL_GETTING_STARTED),
    ]
}

fn run(cli: Cli) -> R<()> {
    JSON_OUTPUT.store(cli.json, std::sync::atomic::Ordering::Relaxed);
    match cli.cmd {
        Cmd::Init { dir, force } => {
            let root = std::path::Path::new(&dir);
            for (rel, contents) in scaffold_files() {
                let path = root.join(rel);
                if path.exists() && !force {
                    return Err(format!("已存在 {}(加 --force 覆盖)", path.display()).into());
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, contents)?;
                println!("写入 {}", path.display());
            }
            println!(
                "下一步:编辑 requirements/example.md,然后 `shepherd login` 并按其中命令录入需求。"
            );
        }
        Cmd::Login { url, user, password } => {
            let mut cfg = Config::load();
            cfg.url = url;
            let client = Client::new(cfg.clone())?;
            let v = client.post(
                "/auth/login",
                json!({"username": user, "password": password}),
                false,
            )?;
            let token = v["token"].as_str().ok_or("登录响应缺少 token")?;
            cfg.token = token.to_string();
            cfg.save()?;
            println!("✅ 已登录 {} → 会话存于 {}", cfg.url, config_path().display());
        }
        Cmd::Agent { cmd } => match cmd {
            AgentCmd::Connect { kind } => {
                let executor = normalize_agent(&kind)?;
                let mut cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                cfg.agent = Some(executor.clone());
                cfg.save()?;
                println!(
                    "✅ 已连接 agent: {executor}  服务 {} {}",
                    cfg.url,
                    if healthy { "(可达)" } else { "(暂不可达)" }
                );
            }
            AgentCmd::Status => {
                let cfg = Config::load();
                let healthy = Client::new(cfg.clone())?.get("/healthz", false).is_ok();
                println!("服务  : {}", cfg.url);
                println!("登录  : {}", if cfg.token.is_empty() { "未登录" } else { "已登录" });
                println!("agent : {}", cfg.agent.as_deref().unwrap_or("(未连接,默认 CLAUDE_CODE)"));
                println!("健康  : {}", if healthy { "可达" } else { "不可达" });
            }
            AgentCmd::Disconnect => {
                let mut cfg = Config::load();
                cfg.agent = None;
                cfg.save()?;
                println!("已断开 agent 连接(dispatch 回落默认 CLAUDE_CODE)");
            }
        },
        Cmd::Req { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ReqCmd::Add { project, title, description, criteria } => pretty(&c.post(
                    "/requirement",
                    json!({"projectId": project, "title": title, "description": description, "acceptanceCriteria": criteria}),
                    true,
                )?),
                ReqCmd::List { project, page, page_size } => pretty(&c.get(
                    &format!("/requirement?projectId={project}&current={page}&pageSize={page_size}"),
                    true,
                )?),
                ReqCmd::Breakdown { req, version, ai: _ } => {
                    let path = match version {
                        Some(v) => format!("/requirement/{req}/breakdown?version={v}"),
                        None => format!("/requirement/{req}/breakdown"),
                    };
                    pretty(&c.post(&path, json!({}), true)?);
                }
            }
        }
        Cmd::Decompose { req, version } => {
            let c = Client::new(Config::load())?;
            pretty(&c.post(
                "/decomposition",
                json!({"requirementId": req, "requirementVersion": version}),
                true,
            )?);
        }
        Cmd::Decomposition { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                DecompositionCmd::Get { id } => {
                    pretty(&c.get(&format!("/decomposition/{id}"), true)?)
                }
                DecompositionCmd::Ready { id } => {
                    pretty(&c.get(&format!("/decomposition/{id}/ready"), true)?)
                }
                DecompositionCmd::Run { id, executor, concurrency } => pretty(&c.post(
                    &format!("/decomposition/{id}/run"),
                    json!({"executor": executor, "maxConcurrency": concurrency}),
                    true,
                )?),
            }
        }
        Cmd::Task { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                TaskCmd::Add { decomp, title, deps } => pretty(&c.post(
                    &format!("/decomposition/{decomp}/task"),
                    json!({"title": title, "dependencies": deps}),
                    true,
                )?),
            }
        }
        Cmd::Dispatch { decomp, task, title, executor, instructions, project, skills } => {
            let cfg = Config::load();
            let exec =
                executor.or_else(|| cfg.agent.clone()).unwrap_or_else(|| "CLAUDE_CODE".into());
            let c = Client::new(cfg)?;
            let mut instr = instructions;
            if !skills.is_empty() {
                let project = project.ok_or("--skills 需配合 --project")?;
                let comp = c.post(
                    "/skill/compose",
                    json!({"projectId": project, "skillIds": skills}),
                    true,
                )?;
                let composed = comp["instructions"].as_str().unwrap_or("").to_string();
                instr = Some(match instr {
                    Some(extra) if !extra.trim().is_empty() => format!("{composed}\n\n{extra}"),
                    _ => composed,
                });
            }
            pretty(&c.post(
                "/delivery",
                json!({"decompositionId": decomp, "taskId": task, "title": title, "executor": exec, "instructions": instr}),
                true,
            )?);
        }
        Cmd::Verify { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                VerifyCmd::Create { req, version, criteria } => pretty(&c.post(
                    "/verification",
                    json!({"requirementId": req, "requirementVersion": version, "criteria": criteria}),
                    true,
                )?),
                VerifyCmd::Link { id, criterion, decomp, task } => pretty(&c.post(
                    &format!("/verification/{id}/link"),
                    json!({"criterionIndex": criterion, "decompositionId": decomp, "taskId": task}),
                    true,
                )?),
                VerifyCmd::Sync { id, decomp, task, unsatisfied } => pretty(&c.post(
                    &format!("/verification/{id}/sync"),
                    json!({"decompositionId": decomp, "taskId": task, "satisfied": !unsatisfied}),
                    true,
                )?),
                VerifyCmd::Report { id } => pretty(&c.get(&format!("/verification/{id}/report"), true)?),
            }
        }
        Cmd::Skill { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                SkillCmd::Add { project, name, instructions, includes } => pretty(&c.post(
                    "/skill",
                    json!({"projectId": project, "name": name, "instructions": instructions, "includes": includes}),
                    true,
                )?),
                SkillCmd::Compose { project, ids } => pretty(&c.post(
                    "/skill/compose",
                    json!({"projectId": project, "skillIds": ids}),
                    true,
                )?),
            }
        }
        Cmd::Logout => {
            let mut cfg = Config::load();
            let c = Client::new(cfg.clone())?;
            let _ = c.post("/auth/logout", json!({}), true);
            cfg.token.clear();
            cfg.save()?;
            println!("✅ 已注销,本地会话已清空");
        }
        Cmd::Project { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ProjectCmd::Create { org, name } => pretty(&c.post(
                    "/project",
                    json!({"organizationId": org, "name": name}),
                    true,
                )?),
                ProjectCmd::List { org, current, page_size } => pretty(&c.get(
                    &format!(
                        "/project?organizationId={org}&current={current}&pageSize={page_size}"
                    ),
                    true,
                )?),
            }
        }
        Cmd::Bug { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                BugCmd::Create { project, title, status } => pretty(&c.post(
                    "/bug",
                    json!({"projectId": project, "title": title, "initialStatus": status}),
                    true,
                )?),
                BugCmd::Status { id, to } => {
                    pretty(&c.post(&format!("/bug/{id}/status"), json!({"status": to}), true)?)
                }
            }
        }
        Cmd::Case { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                CaseCmd::Review { review, case, reviewer, status, content } => pretty(&c.post(
                    &format!("/case-review/{review}/{case}"),
                    json!({"reviewerId": reviewer, "status": status, "content": content}),
                    true,
                )?),
            }
        }
        Cmd::Plan { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PlanCmd::Create { project, name, plan_type, group } => {
                    let mut body = json!({"projectId": project, "name": name, "type": plan_type});
                    if let Some(g) = group {
                        body["groupId"] = json!(g);
                    }
                    pretty(&c.post("/test-plan", body, true)?)
                }
                PlanCmd::Stats { id } => {
                    pretty(&c.get(&format!("/test-plan/{id}/statistics"), true)?)
                }
                PlanCmd::Report { id, markdown } => {
                    let path = if markdown {
                        format!("/test-plan/{id}/report.md")
                    } else {
                        format!("/test-plan/{id}/report")
                    };
                    print!("{}", c.get_text(&path, false)?)
                }
                PlanCmd::Schedule { id, cron } => pretty(&c.post(
                    &format!("/test-plan/{id}/schedule"),
                    json!({"cron": cron}),
                    true,
                )?),
                PlanCmd::Runs { id } => pretty(&c.get(&format!("/test-plan/{id}/runs"), true)?),
                PlanCmd::LinkCase { id, case, name } => pretty(&c.post(
                    &format!("/test-plan/{id}/cases"),
                    json!({"caseId": case, "name": name}),
                    true,
                )?),
                PlanCmd::Result {
                    id,
                    case,
                    status,
                    latency_ms,
                    status_code,
                    body,
                    assertions_json,
                } => {
                    let assertions: Value = match assertions_json {
                        Some(aj) => serde_json::from_str(&aj)
                            .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                        None => json!([]),
                    };
                    pretty(&c.post(
                        &format!("/test-plan/{id}/cases/{case}/result"),
                        json!({"status": status, "latencyMs": latency_ms, "statusCode": status_code,
                               "body": body, "assertions": assertions}),
                        true,
                    )?)
                }
                PlanCmd::Cases { id } => pretty(&c.get(&format!("/test-plan/{id}/cases"), true)?),
                PlanCmd::Run { id } => {
                    pretty(&c.post(&format!("/test-plan/{id}/run"), json!({}), true)?)
                }
            }
        }
        Cmd::Api { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApiCmd::BatchRun { project, cases, run_mode, pool, env } => pretty(&c.post(
                    "/api/batch-run",
                    json!({"projectId": project, "caseIds": cases, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
            }
        }
        Cmd::User { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                UserCmd::Create { name, email } => {
                    pretty(&c.post("/system/user", json!({"name": name, "email": email}), true)?)
                }
                UserCmd::List { current, page_size } => pretty(
                    &c.get(&format!("/system/user?current={current}&pageSize={page_size}"), true)?,
                ),
                UserCmd::Get { id } => pretty(&c.get(&format!("/system/user/{id}"), true)?),
                UserCmd::Update { id, name, email, disable } => pretty(&c.put(
                    &format!("/system/user/{id}"),
                    json!({"name": name, "email": email, "enable": !disable}),
                    true,
                )?),
                UserCmd::Delete { id } => pretty(&c.delete(&format!("/system/user/{id}"), true)?),
                UserCmd::Names { ids } => {
                    pretty(&c.get(&format!("/system/user/names?ids={}", ids.join(",")), true)?)
                }
            }
        }
        Cmd::Org { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                OrgCmd::Create { name, disable } => pretty(&c.post(
                    "/organization",
                    json!({"name": name, "enable": !disable}),
                    true,
                )?),
                OrgCmd::List { current, page_size } => pretty(
                    &c.get(&format!("/organization?current={current}&pageSize={page_size}"), true)?,
                ),
                OrgCmd::Get { id } => pretty(&c.get(&format!("/organization/{id}"), true)?),
                OrgCmd::Update { id, name, disable } => pretty(&c.put(
                    &format!("/organization/{id}"),
                    json!({"name": name, "enable": !disable}),
                    true,
                )?),
                OrgCmd::Delete { id } => pretty(&c.delete(&format!("/organization/{id}"), true)?),
            }
        }
        Cmd::Role { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                RoleCmd::Create { name, scope, permissions } => pretty(&c.post(
                    "/role",
                    json!({"name": name, "scope": scope, "permissions": permissions}),
                    true,
                )?),
                RoleCmd::List { current, page_size } => {
                    pretty(&c.get(&format!("/role?current={current}&pageSize={page_size}"), true)?)
                }
                RoleCmd::Get { id } => pretty(&c.get(&format!("/role/{id}"), true)?),
                RoleCmd::Update { id, name, scope, permissions } => pretty(&c.put(
                    &format!("/role/{id}"),
                    json!({"name": name, "scope": scope, "permissions": permissions}),
                    true,
                )?),
                RoleCmd::Delete { id } => pretty(&c.delete(&format!("/role/{id}"), true)?),
                RoleCmd::Grant { user, role } => pretty(&c.post(
                    "/user-role/grant",
                    json!({"userId": user, "roleId": role}),
                    true,
                )?),
                RoleCmd::Revoke { user, role } => pretty(&c.post(
                    "/user-role/revoke",
                    json!({"userId": user, "roleId": role}),
                    true,
                )?),
            }
        }
        Cmd::Apidef { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ApidefCmd::Create { project, name, protocol, method, path } => pretty(&c.post(
                    "/api/definition",
                    json!({"projectId": project, "name": name, "protocol": protocol, "method": method, "path": path}),
                    true,
                )?),
                ApidefCmd::Import { project, file, url } => {
                    let raw = match (url, file) {
                        (Some(u), _) => c.fetch_text(&u)?,
                        (None, Some(f)) => std::fs::read_to_string(&f)?,
                        (None, None) => return Err("需指定 --file 或 --url".into()),
                    };
                    let content: Value =
                        serde_json::from_str(&raw).map_err(|e| format!("导入内容不是合法 JSON: {e}"))?;
                    pretty(&c.post(
                        "/api/definition/import",
                        json!({"projectId": project, "content": content}),
                        true,
                    )?)
                }
                ApidefCmd::List { project } => {
                    pretty(&c.get(&format!("/api/definition?projectId={project}"), true)?)
                }
                ApidefCmd::Get { id } => pretty(&c.get(&format!("/api/definition/{id}"), true)?),
                ApidefCmd::Case { def, name, method, url, body, expect_status } => pretty(&c.post(
                    &format!("/api/definition/{def}/case"),
                    json!({"name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
                    true,
                )?),
                ApidefCmd::Cases { def } => {
                    pretty(&c.get(&format!("/api/definition/{def}/case"), true)?)
                }
                ApidefCmd::CaseNew { project, def, name, method, url, body, expect_status } => pretty(&c.post(
                    "/api/case",
                    json!({"projectId": project, "apiDefinitionId": def, "name": name, "method": method, "url": url, "body": body, "assertions": status_assertions(expect_status)}),
                    true,
                )?),
                ApidefCmd::GenSuite { project, base, no_scenario } => {
                    gen_suite(&c, &project, base.trim_end_matches('/'), no_scenario)?
                }
                ApidefCmd::CaseList { project, current, page_size } => pretty(&c.get(
                    &format!("/api/case?projectId={project}&current={current}&pageSize={page_size}"),
                    true,
                )?),
                ApidefCmd::CaseExec { case, current, page_size } => pretty(&c.get(
                    &format!("/api/case/{case}/executions?current={current}&pageSize={page_size}"),
                    true,
                )?),
                ApidefCmd::CaseRun { case, project, run_mode, pool, env } => pretty(&c.post(
                    &format!("/api/case/{case}/run"),
                    json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
                ApidefCmd::Mock { def, name, response_status, body } => pretty(&c.post(
                    &format!("/api/definition/{def}/mock"),
                    json!({"name": name, "responseStatus": response_status, "responseBody": body}),
                    true,
                )?),
                ApidefCmd::Mocks { def } => {
                    pretty(&c.get(&format!("/api/definition/{def}/mock"), true)?)
                }
            }
        }
        Cmd::Scenario { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                ScenarioCmd::Create { project, name } => pretty(&c.post(
                    "/api/scenario",
                    json!({"projectId": project, "name": name}),
                    true,
                )?),
                ScenarioCmd::List { project } => {
                    pretty(&c.get(&format!("/api/scenario?projectId={project}"), true)?)
                }
                ScenarioCmd::Get { id } => pretty(&c.get(&format!("/api/scenario/{id}"), true)?),
                ScenarioCmd::Step { scenario, kind, ref_mode, order, ref_id, method, url, body, expect_status, assertions_json, control_json } => {
                    let mut step = json!({"kind": kind.to_uppercase(), "refMode": ref_mode.to_uppercase(), "order": order});
                    if let Some(r) = ref_id {
                        step["refId"] = json!(r);
                    }
                    if let Some(m) = method {
                        let assertions = match assertions_json {
                            Some(aj) => serde_json::from_str(&aj)
                                .map_err(|e| format!("--assertions-json 不是合法 JSON: {e}"))?,
                            None => status_assertions(expect_status),
                        };
                        step["request"] = json!({"method": m, "url": url.unwrap_or_default(), "body": body, "assertions": assertions});
                    }
                    if let Some(cj) = control_json {
                        step["control"] = serde_json::from_str(&cj)
                            .map_err(|e| format!("--control-json 不是合法 JSON: {e}"))?;
                    }
                    pretty(&c.post(&format!("/api/scenario/{scenario}/step"), step, true)?)
                }
                ScenarioCmd::Compile { id } => {
                    pretty(&c.get(&format!("/api/scenario/{id}/compile"), true)?)
                }
                ScenarioCmd::Executions { id, current, page_size } => pretty(&c.get(
                    &format!("/api/scenario/{id}/executions?current={current}&pageSize={page_size}"),
                    true,
                )?),
                ScenarioCmd::Run { id, project, run_mode, pool, env } => pretty(&c.post(
                    &format!("/api/scenario/{id}/run"),
                    json!({"projectId": project, "runMode": run_mode, "poolId": pool, "environmentId": env}),
                    true,
                )?),
            }
        }
        Cmd::Env { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                EnvCmd::Create { project, name, base, headers, vars } => pretty(&c.post(
                    "/api/environment",
                    json!({"projectId": project, "name": name, "baseUrl": base,
                           "headers": parse_headers(&headers)?, "variables": parse_vars(&vars)?}),
                    true,
                )?),
                EnvCmd::List { project } => {
                    pretty(&c.get(&format!("/api/environment?projectId={project}"), true)?)
                }
                EnvCmd::Get { id } => pretty(&c.get(&format!("/api/environment/{id}"), true)?),
                EnvCmd::Update { id, project, name, base, headers, vars } => pretty(&c.put(
                    &format!("/api/environment/{id}"),
                    json!({"projectId": project, "name": name, "baseUrl": base,
                           "headers": parse_headers(&headers)?, "variables": parse_vars(&vars)?}),
                    true,
                )?),
                EnvCmd::Delete { id } => {
                    pretty(&c.delete(&format!("/api/environment/{id}"), true)?)
                }
            }
        }
        Cmd::Fcase { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                FcaseCmd::Create { project, name, module, priority, status, fields } => pretty(&c.post(
                    "/functional-case",
                    json!({"projectId": project, "name": name, "module": module, "priority": priority,
                           "status": status, "customFields": parse_vars(&fields)?}),
                    true,
                )?),
                FcaseCmd::List { project } => {
                    pretty(&c.get(&format!("/functional-case?projectId={project}"), true)?)
                }
                FcaseCmd::Export { project, out } => {
                    let bytes = c.get_bytes(&format!("/functional-case/export?projectId={project}"), true)?;
                    std::fs::write(&out, &bytes)?;
                    println!("✅ 已导出 {} 字节 → {out}", bytes.len());
                }
                FcaseCmd::Import { project, file } => {
                    let bytes = std::fs::read(&file)?;
                    pretty(&c.post_bytes(
                        &format!("/functional-case/import?projectId={project}"),
                        bytes,
                        true,
                    )?)
                }
            }
        }
        Cmd::Runner { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                RunnerCmd::Register { name, base_url, token } => pretty(&c.post(
                    "/runner-agent",
                    json!({"name": name, "baseUrl": base_url, "token": token}),
                    true,
                )?),
                RunnerCmd::List => pretty(&c.get("/runner-agent", true)?),
                RunnerCmd::Run { agent, url, method, body, expect_status } => pretty(&c.post(
                    &format!("/runner-agent/{agent}/run"),
                    json!({
                        "request": {"method": method.to_uppercase(), "url": url, "headers": [], "body": body},
                        "assertions": status_assertions(expect_status),
                    }),
                    true,
                )?),
                RunnerCmd::RunCase { agent, case } => pretty(&c.post(
                    &format!("/runner-agent/{agent}/run-case"),
                    json!({"caseId": case}),
                    true,
                )?),
                RunnerCmd::Executions { agent } => {
                    pretty(&c.get(&format!("/runner-agent/{agent}/executions"), true)?)
                }
                RunnerCmd::Refresh { agent } => pretty(&c.post(
                    &format!("/runner-agent/{agent}/refresh"),
                    json!({}),
                    true,
                )?),
                RunnerCmd::Probe { protocol, target, payload, meta, expect_status, contains, latency_under } => {
                    let mut metadata = serde_json::Map::new();
                    for kv in &meta {
                        if let Some((k, v)) = kv.split_once('=') {
                            metadata.insert(k.to_string(), json!(v));
                        }
                    }
                    let mut assertions: Vec<Value> = Vec::new();
                    if let Some(s) = expect_status {
                        assertions.push(json!({"type": "status_is", "value": s}));
                    }
                    if let Some(sub) = &contains {
                        assertions.push(json!({"type": "output_contains", "value": sub}));
                    }
                    if let Some(ms) = latency_under {
                        assertions.push(json!({"type": "latency_under_ms", "value": ms}));
                    }
                    pretty(&c.post(
                        "/runner/probe",
                        json!({
                            "protocol": protocol, "target": target, "payload": payload,
                            "metadata": metadata, "assertions": assertions,
                        }),
                        true,
                    )?)
                }
            }
        }
        Cmd::Perf { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PerfCmd::Run {
                    url,
                    method,
                    concurrency,
                    iterations,
                    duration_ms,
                    expect_status,
                    contains,
                    equals,
                    latency_under,
                    protocol,
                    query,
                    project,
                } => pretty(&c.post(
                    "/perf/run",
                    json!({"url": url, "method": method, "concurrency": concurrency,
                               "iterations": iterations, "durationMs": duration_ms,
                               "expectStatus": expect_status, "expectContains": contains,
                               "expectEquals": equals, "latencyUnderMs": latency_under,
                               "protocol": protocol, "query": query,
                               "projectId": project}),
                    true,
                )?),
                PerfCmd::Report { id } => pretty(&c.get(&format!("/perf/report/{id}"), true)?),
            }
        }
        Cmd::Pool { cmd } => {
            let c = Client::new(Config::load())?;
            match cmd {
                PoolCmd::Create { name, disable } => pretty(&c.post(
                    "/api/resource-pool",
                    json!({"name": name, "enabled": !disable}),
                    true,
                )?),
                PoolCmd::List => pretty(&c.get("/api/resource-pool", true)?),
            }
        }
    }
    Ok(())
}

fn gen_suite(c: &Client, project: &str, base: &str, no_scenario: bool) -> R<()> {
    let defs = c.get(&format!("/api/definition?projectId={project}"), true)?;
    let list = defs.as_array().ok_or("接口定义列表不是数组")?;
    if list.is_empty() {
        return Err("项目内没有接口定义,先 `apidef import`".into());
    }
    let (mut cases, mut scenarios, mut steps, mut failed) = (0u32, 0u32, 0u32, 0u32);
    for d in list {
        let def_id = d["id"].as_str().unwrap_or_default();
        let name = d["name"].as_str().unwrap_or("(unnamed)");
        let method = d["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = d["path"].as_str().unwrap_or("");
        let url = format!("{base}{path}");
        let success_code = if method == "POST" { 201 } else { 200 };

        let mk_case = |label: &str, code: u16| {
            c.post(
                "/api/case",
                json!({
                    "projectId": project,
                    "apiDefinitionId": def_id,
                    "name": format!("{name} [{label}]"),
                    "method": method,
                    "url": url,
                    "body": Value::Null,
                    "assertions": [{"type": "StatusIs", "args": code}],
                }),
                true,
            )
        };
        let ok = match mk_case(&format!("成功·期望{success_code}"), success_code) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ {name}: 成功用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let bad = match mk_case("失败·期望401", 401) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ {name}: 失败用例创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        cases += 2;
        let (ok_id, bad_id) = (
            ok["id"].as_str().unwrap_or_default().to_string(),
            bad["id"].as_str().unwrap_or_default().to_string(),
        );

        if no_scenario {
            continue;
        }
        let sc = match c.post(
            "/api/scenario",
            json!({"projectId": project, "name": format!("{name} 场景")}),
            true,
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("✗ {name}: 场景创建失败:{e}");
                failed += 1;
                continue;
            }
        };
        let sc_id = sc["id"].as_str().unwrap_or_default();
        scenarios += 1;
        for (order, ref_id) in [(1, &ok_id), (2, &bad_id)] {
            match c.post(
                &format!("/api/scenario/{sc_id}/step"),
                json!({"kind": "CASE", "refMode": "REFERENCE", "order": order, "refId": ref_id}),
                true,
            ) {
                Ok(_) => steps += 1,
                Err(e) => eprintln!("✗ {name}: 步骤{order}添加失败:{e}"),
            }
        }
    }
    println!(
        "✅ 生成完成:{} 个接口 → {cases} 条用例、{scenarios} 条场景、{steps} 个步骤(失败 {failed})",
        list.len()
    );
    Ok(())
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("✗ {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn normalize_agent_maps_kinds() {
        assert_eq!(normalize_agent("claude-code").unwrap(), "CLAUDE_CODE");
        assert_eq!(normalize_agent("CODEX").unwrap(), "CODEX");
        assert_eq!(normalize_agent("opencode").unwrap(), "OPENCODE");
        assert_eq!(normalize_agent("CodeBuddy").unwrap(), "CODEBUDDY");
        assert_eq!(normalize_agent("code-buddy").unwrap(), "CODEBUDDY");
        assert!(normalize_agent("gpt").is_err());
    }

    #[test]
    fn url_join_trims_slash() {
        let c = Client::new(Config { url: "http://h:1/".into(), token: "t".into(), agent: None })
            .expect("client");
        assert_eq!(c.url("/x"), "http://h:1/x");
    }

    #[test]
    fn config_defaults_url_when_empty() {
        let c = Config { url: String::new(), token: String::new(), agent: None };
        assert!(c.url.is_empty());
    }

    #[test]
    fn scaffold_templates_carry_real_commands() {
        let files = scaffold_files();
        assert_eq!(files.len(), 2);
        let req = files.iter().find(|(p, _)| *p == "requirements/example.md").unwrap().1;
        assert!(req.contains("shepherd req add"));
        assert!(req.contains("--criteria"));
    }

    #[test]
    fn init_writes_scaffold_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!("shep-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let init = |force: bool| {
            run(Cli { json: false, cmd: Cmd::Init { dir: dir.to_string_lossy().into(), force } })
        };
        init(false).expect("first init writes");
        assert!(dir.join("requirements/example.md").is_file());
        assert!(dir.join("shepherd.getting-started.md").is_file());
        // 二次无 --force:拒绝覆盖。
        assert!(init(false).is_err());
        // --force:覆盖成功。
        init(true).expect("force overwrites");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
