# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows SemVer.
Unreleased changes go under Unreleased and move into the matching version section on release.

> 简体中文版见 [CHANGELOG.zh-CN.md](CHANGELOG.zh-CN.md).

## [Unreleased]

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
