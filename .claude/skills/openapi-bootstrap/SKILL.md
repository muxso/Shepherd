---
name: openapi-bootstrap
description: Self-bootstrap (dogfood) Shepherd's own HTTP API. Fetches the running system's OpenAPI (/api-docs/openapi.json), idempotently imports it as API definitions (with parsed request params / required flags / responses + a default assertion-bearing case per interface), then builds a REAL chained scenario that references api cases (kind=CASE): login → extract token → authenticated calls (Bearer ${token}) → extract orgId → chained call, plus a negative 401 case. Runs it in-process with an environment and reports per-step pass/fail + extracted variables. A companion script (scenarios_all.py) builds one real CRUD/lifecycle scenario per business module (every OpenAPI tag — org, project, requirement, task, delivery, verification, test-plan, etc.) and executes all of them with a per-module pass/fail report (currently 20/20 modules green). Use when the user asks to 自举 / dogfood / smoke-test the live API, to get scenario coverage across all business modules (所有业务模块的场景测试), or to orchestrate the system's own OpenAPI through /api/definition and /api/scenario.
---

# OpenAPI 自举(Self-Bootstrap)

用 Shepherd 自己的「接口定义 + 用例 + 场景编排 + 执行」能力,测试 Shepherd 自己运行中的 HTTP API。

## 运行

```bash
# A. 单链路自举(登录→提取→鉴权链 + 负向 401):验证「真链路」执行机制
python3 .claude/skills/openapi-bootstrap/selftest.py

# B. 全业务模块场景覆盖:每个 OpenAPI tag 一条 CRUD/生命周期场景,逐模块执行报告
python3 .claude/skills/openapi-bootstrap/scenarios_all.py
```

`scenarios_all.py` 为 20 个业务模块各建一条真实链路场景(引用接口用例 kind=CASE,带鉴权头 / 变量提取 / 断言),
按 CRUD/生命周期串起并带环境进程内执行,失败策略 CONTINUE → 一次跑出所有模块逐步通过/失败 + 原因。
当前全绿:**20/20 模块,114/114 步**。无「改用例」端点 → 每次删旧 holder 定义(级联其用例)再重建,
持久资源(项目/用户/需求)名字带本次运行唯一短串,可重复运行不撞唯一约束。

构建过程中印证的真实约束(写死在 chain 里,改后端枚举须同步):
- 资源池 `poolType` ∈ {`Node`,`Kubernetes`}(非 LOCAL);`allOrg=false` 时 `orgIds` 必须非空。
- 任务创建 `POST /decomposition/{id}/task` 返回 `{"taskId": <slug>}`(非 `id`);点数/状态端点用该 taskId。
- 交付执行器 `CLAUDE_CODE`/`CODEX` 为**同步 stub**:`POST /delivery` 一步到位,创建即 `DELIVERED`(无 running/complete 异步转移)。
- `/runner/probe`、`/runner-agent/{id}/run` 需**在线 runner agent**(无则 502),本地链只覆盖管理面。
- 多个 list 端点必须带 `?projectId=`(requirement / skill / functional-case / case-review)。
- 创建走 201、删除/部分改走 204 → 用 `ResponseCode < 400` 通用成功断言,而非精确 `StatusIs(200)`。

| 环境变量 | 默认 | 说明 |
|------|------|------|
| `SHEPHERD_BASE` | `http://127.0.0.1:9180` | 后端地址 |
| `SHEPHERD_USER` / `SHEPHERD_PASS` | `admin` / `s3cret` | 登录凭据(`MS_ADMIN_PASSWORD`) |
| `SHEPHERD_PROJECT_ID` | 自动解析 | 指定项目;缺省取第一个组织的第一个项目,没有则创建 |

脚本与环境/定义/用例/场景均按名字「建或复用」,可重复运行不堆积。

## 流程(7 步)

1. 登录拿 token(仅用于建资源的管理调用)
2. 解析/创建 组织 → 项目
3. 抓本系统 OpenAPI(`GET /api-docs/openapi.json`)→ **幂等导入**(`POST /api/definition/import`,同 方法+路径 覆盖 spec,不重复堆积);并抽样核验 spec 已填充 + 每个接口带断言用例
4. 建/复用 指向本机的环境(`baseUrl = 本服务`)
5. 建/复用 「自举链路」定义下的 4 个**真实用例**
6. 建/复用 场景,**引用**这些用例为步骤(`kind=CASE`),按序对齐后带环境执行
7. 拉报告,逐步打印 ✅/❌ + 提取变量 + 断言通过数

## 这是真链路,不是写死的 GET

场景**引用接口用例**(`kind=CASE`),而非内联硬编码请求。用例覆盖 POST + GET + 鉴权头 + 变量提取 + 跨步链路:

```
① 登录并提取token   POST /auth/login   断言 状态200 + 含 token   → 提取 token = $.token
② 鉴权列组织        GET  /organization  头 Authorization: Bearer ${token}  → 提取 orgId = $.items[0].id
③ 鉴权按orgId列项目  GET  /project?organizationId=${orgId}  (token+orgId 双变量代入)
④ 负向·无token访问被拒 GET /organization  断言 状态401
```

关键执行机制(见 `crates/api-test/src/adapters/{plan,local,pg}.rs`、`crates/api-runner/src/domain/runner.rs`):
- **CASE 步骤进程内执行**,加载用例完整 method/url/body/headers/auth/assertions/processors —— 无需资源池(内联 `REQUEST` 步骤才不带头,故用 CASE)。
- **EXTRACT 处理器**把 `$.token` 等写入运行变量,跨步骤传递。
- **`${var}` 单花括号**替换作用于 url / 头值 / body(注意不是 `{{}}`)。
- **环境** baseUrl 对相对 url 自动前缀拼接;默认头按缺补充(用例同名头优先)。
- 已知限制:`Variable` 类断言在场景内拿不到运行变量(`plan.rs` 未把 vars 传给断言求值),故本脚本用 `StatusIs`/`BodyContains` 而不用 `Variable` 断言。

## 后端导入器增强(本次随附)

`POST /api/definition/import` 现在:
- 解析每个 operation 的 `parameters`(query/header/path,必填编码进备注)、`requestBody`(`$ref`/`allOf` 按 `components` 解析成 bodySchema 树 + 示例)、`responses`(状态码 + schema 示例)→ 写入定义 `spec`;
- 为每个**新**接口生成默认用例(状态码 + 基础业务断言);
- **幂等**:同 项目+方法+路径 已存在则只覆盖其 spec(保留用户已编辑的用例),返回 `{created, updated, skipped}`。

代码:`crates/api-definition/src/domain/import.rs`、`application/import_api_definitions.rs`、`domain/api_definition.rs`(`with_spec`)。

## 退出码
`0` 全绿;`1` 有步骤失败;`2` 自举流程本身报错。
