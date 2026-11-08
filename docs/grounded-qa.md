[简体中文](grounded-qa.zh-CN.md) · English

# Grounded project Q&A

Web chat and the WeCom connector share `/rag/ask/stream`. Structured business queries read project records directly; document questions use the caller's visible knowledge sources. Business counts are calculated by the server, rather than inferred from retrieved passages.

## Available data

The read catalog covers requirements, bugs, functional cases, test plans and runs, case reviews, API definitions and cases, API scenarios and executions, performance reports, tasks, deliveries, design proposals, verification records, test services, functional scenarios and runs, interface inventories, requirement impact reviews, file metadata, project members, skills metadata, templates, and the current project. File bytes, environment credentials, API authentication settings, and executable skill instructions are excluded from this catalog.

Examples:

- `项目概览：需求、缺陷、功能用例各有多少？`
- `功能用例按服务统计图`
- `最近 30 天失败的接口执行报告有哪些？`
- `查询名称包含“支付”的需求`
- `云效最近 7 天新增缺陷统计图`
- `每个组本周的代码热力图`

Queries retain project membership and resource read permissions. The planner can select only registered resources and validated filters; it cannot supply SQL, URLs, or executable operations. Read transactions are time-limited. Totals include all matching records; displayed details are limited to twelve rows. Large group distributions show at most one hundred buckets. Date filters use Shanghai time and disclose missing timestamps. External data remains limited by collection coverage.

Git activity does not establish work hours or performance. A daily author summary does not prove who edited a dependency or caused a production failure. Such attribution requires the actual diff and, for production causality, deployment and runtime evidence. Failed or unauthorized reads never become zero counts.

## WeCom authorization

The administrator configures channels in **System parameters → RAG**. Enable the channel, choose its dedicated service account, and add project-scoped group or direct-user associations. Group binding has a separate switch and is off by default. When enabled, an administrator can issue an expiring, single-use project binding code. No wildcard conversation authorization is supported.

Enable `WECOM_REMOTE_POLICY=1` on the connector to enforce backend policy. Existing local bindings must be migrated explicitly before switching. Heartbeat status expires after ninety seconds; the page distinguishes an enabled authorization from a live connection. The connector records pending conversation identifiers without message text. See the [connector guide](../plugins/wecom-bot/README.md).

## Maintained onboarding knowledge

Use **Onboarding knowledge** in chat or RAG settings to maintain project guides and FAQs. Enter verified content, a source URL when available, and visibility groups. An empty group selection restricts a document to its maintainer and administrators. To share it through the bot, select a visibility group available to the connector account's role.

Publishing replaces the searchable document and saves a numbered revision. Concurrent stale edits are rejected. Onboarding and company-policy questions search maintained onboarding documents under the current audience. No company policies are prefilled. Missing or invisible material must be reported as insufficient evidence. Citations link to a recorded source URL or an authorized internal document view, with an event or publication date when known.
