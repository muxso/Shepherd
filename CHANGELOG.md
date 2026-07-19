# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows SemVer.
Unreleased changes go under Unreleased and move into the matching version section on release.

> 简体中文版见 [CHANGELOG.zh-CN.md](CHANGELOG.zh-CN.md).

## [Unreleased]

## [0.0.4] - 2026-07-19

### Added
- Remote execution over WebSocket: pool runners dial out and register into resource pools (by pool name or id) with heartbeat/reconnect; scenario, union-batch and plan runs route to connected runners and stream per-step events back; browser live-run view animates step status and timings from the same stream; per-runner concurrency caps with pool-side queueing; capability tag matching; graceful in-process fallback everywhere
- Resource pools reworked around runners: Node pools drop the manual IP/Port node table for a join-command panel plus a live online-runner list; single runs auto-pick an applicable pool (explicit poolId still wins) and responses report where they executed; live pool-name uniqueness
- In-app notification service: personal inbox (categories, @me/unread/read tabs, unread badge, click-through navigation) fed by real events — bug assignment/status change, review creation, comment @mentions, plan run finished, scenario run failed, scheduled run failures
- Notification rules and webhook robots: per-event channel routing with templates, Feishu / DingTalk (HMAC signing) / WeCom payload formats, test-send, message settings page persisted server-side
- Test plans: server-side list with created_by (frontend registry retired; delete = archive), async single-case runs with live row status, plan-scoped bug linkage (bugs tab with link/create/unlink; case drawer plan-bugs view), manual runs recorded into the execution history
- Bugs gain severity (P0-P3), handler and update audit fields end to end
- Follow (关注) entries for API cases, scenarios and case reviews; six-section follows workbench
- Case reviews: delete endpoint; planning mind-map prunes un-executed links when nodes are removed
- Scenario batch export to JSON; legacy base64 image migration script; execution endpoints exempt from the global 30s timeout (600s)

### Fixed
- Manual plan runs now appear in the execution history; re-saved plan schedules no longer stack cron jobs; ghost schedules stopped firing after plan deletion
- Bug creation from the case drawer sent an invalid initial status and silently failed

## [0.0.3] - 2026-07-18

### Added
- Functional case detail drawer (tabs for detail/history/bugs/reviews/plans/dependencies, inline content editing, follow and comments)
- Scenario batch operations: numeric IDs, sortable columns, bottom batch bar, batch run dialog with union reports and resource-pool admission/concurrency limits
- Scenario recycle bin (soft delete keeps steps; restore and purge, batch operations)
- Import-request dialog with copy vs reference semantics: copies are fully editable inline requests with provenance tags; scenario copy mounts an independent copy group; referenced-data refresh button
- HTTP phase timings (DNS / TTFB / download) captured per request, shown as a Chrome-style timing breakdown shared across reports, step results and the debug console
- Runtime variables `${__runid}` / `${__timestamp}` make persistent-resource chains re-runnable; full-module self-regression stays green across repeated runs
- Test plans: scenarios mountable next to cases; plan execution unified through the scenario runner with per-case scenario reports; plan detail page with planning mind-map (test points, config leaves, case-link dialog), cases tab with test-point/module tree and per-case run/unlink, header and tabs; create/edit right drawers; schedule delete endpoint
- Case review meta (name, description, tags, module, review period) and a reworked review list page with module tree and edit drawer
- Workbench-style todos and follows pages (stacked module cards; six-section follows)
- Login page redesign (sliding two-panel, animated logo); markdown editor for skill instructions
- openapi-bootstrap skill translated to English plus a test-plan operations self-check script

### Changed
- Flat elevation across the UI: lighter shadows and masks, drag-resizable right drawers everywhere
- README screenshots replaced with single-image day/night diagonal composites

### Fixed
- Plan-mounted scenarios no longer fail from missing runtime variables (plan runs delegate to the scenario runner)
- Re-saving a plan schedule replaces the in-memory cron job instead of stacking; deleted plans no longer leave ghost jobs firing
- Dev-server proxy covers `/follow` and `/comment`; README status badge anchor

## [0.0.2] - 2026-07-14

### Added
- Defect ↔ requirement / scenario-case / functional-case linkage (traceability chain), including a linkage drawer UI
- Dispatch can target a specific registered runtime (by name; under Redis a dedicated stream is used, offline-target tasks stay queued)
- Project member / user-group management wired to the backend (add member, create group, remove)
- Per-runtime API key (`sak_` prefix, argon2 storage, 60s validation cache, revocable), including a web management page and the agent-runtime `SHEPHERD_AGENT_KEY` static-credential mode
- Login failure lockout: 5 consecutive failures for the same username locks for 15 minutes (HTTP 429)
- SECURITY.md / CONTRIBUTING.md / LICENSE (GPL-2.0) / CHANGELOG

### Changed
- All read endpoints now require authentication + `READ` permission (previously ~50 GET endpoints were anonymously accessible)
- Rate limiting on by default (200 rps per client; `SHEPHERD_RATE_LIMIT_RPS=0` to disable)
- server and agent-runtime warn at startup when a weak default admin password is detected
- Web palette changed to a clean light-blue (Arco style); default dark theme is neutral dark-gray
- `publish = false` for all workspace members (shared crate names are not published to crates.io)

### Fixed
- Mutex poisoning now recovers instead of cascading panics (rate limiter / metrics / session on the request path)
- Left-side nav overflow under English locale; clippy fixes including test timeout paths not reaping child processes
