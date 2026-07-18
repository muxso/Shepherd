# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows SemVer.
Unreleased changes go under Unreleased and move into the matching version section on release.

> 简体中文版见 [CHANGELOG.zh-CN.md](CHANGELOG.zh-CN.md).

## [Unreleased]

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
