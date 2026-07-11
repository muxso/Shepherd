# Contributing / 贡献指南

Issues 与 PR 都欢迎。下面是让 PR 顺利合入的最短路径。
Issues and PRs are welcome. Here is the shortest path to a mergeable PR.

## 本地环境 / Local setup

```bash
# Rust 工具链以 CI 为准(.github/workflows/ci.yml 里的 RUST_TOOLCHAIN)
rustup toolchain install 1.86.0 --component clippy rustfmt

# 后端:需要 PostgreSQL(集成测试与本地运行)
docker run -d --name shepherd-pg -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass \
  -e POSTGRES_DB=mstest -p 55432:5432 postgres:16-alpine
cargo run                      # 默认连 localhost:55432,迁移自动执行

# 前端
cd web && npm ci && npm run dev   # Vite 代理到 127.0.0.1:9180
```

## 提交前自检 / Pre-flight checks

CI 会跑这四件事,本地先过一遍能省一个来回:

```bash
cargo fmt --all --check
cargo clippy --workspace --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::unwrap-used
cargo test --workspace --all-features --locked
```

需要真库的集成测试(CI 的 integration job 会跑):

```bash
cargo test -p server --test scenarios -- --ignored --test-threads=1
```

## 架构约定 / Architecture conventions

- 每个业务 crate 采用六边形分层:`domain` / `application` / `ports` / `adapters`。
  **domain、application、ports 禁止任何 IO 依赖**(sqlx/axum/reqwest),各 crate 的
  `tests/architecture.rs` 会强制检查;新业务 crate 请一并复制这个守卫。
- IO 适配器(pg/http)必须在 optional feature 后面;只有 `crates/server`(组装根)
  显式开启它们。
- 新增 HTTP 端点:所有 handler 必须带 `AuthUser` 并做 `user.can(RESOURCE, ACTION)`
  权限检查(公开的认证端点除外);同时在该文件的 `openapi()` 中注册,并保持
  `security(("bearer" = []))` 标注。
- 数据库迁移放在 `crates/migrate/migrations/NNNN_name.sql`,版本号连续不重号
  (重号会被守卫测试拦下);改完执行 `touch crates/migrate/src/lib.rs` 再编译。
- Mutex 统一用 `.lock().unwrap_or_else(std::sync::PoisonError::into_inner)`,
  不要 `.expect("lock")`。

## 提交与 PR / Commits and PRs

- Commit message 用 conventional 风格小写单行:`feat(delivery): …` / `fix(web): …`。
- 一个 PR 做一件事;带上测试;UI 改动附截图。
- PR 描述里写清楚动机与验证方式。

## English summary

Hexagonal layering per business crate (domain/application/ports/adapters, no IO
in pure layers — enforced by `tests/architecture.rs`); IO adapters behind cargo
features; every HTTP handler needs `AuthUser` + a `user.can(...)` check and an
`openapi()` registration; migrations are sequentially numbered under
`crates/migrate/migrations`; run the four CI checks above before pushing;
conventional single-line commit messages.
