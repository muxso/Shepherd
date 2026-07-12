# 贡献指南

> English: [CONTRIBUTING.md](CONTRIBUTING.md).

欢迎提交 Issue 和 PR。下面是最短路径,让 PR 能够顺利合并。

## 本地环境

```bash
# Rust 工具链版本以 CI 为准(.github/workflows/ci.yml 中的 RUST_TOOLCHAIN)
rustup toolchain install 1.86.0 --component clippy rustfmt

# 后端:需要 PostgreSQL(集成测试与本地运行都依赖它)
docker run -d --name shepherd-pg -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass \
  -e POSTGRES_DB=mstest -p 55432:5432 postgres:16-alpine
cargo run                      # 默认连接 localhost:55432,迁移会自动执行

# 前端
cd web && npm ci && npm run dev   # Vite 代理到 127.0.0.1:9180
```

## 提交前检查

CI 会跑下面四项;本地先跑一遍,省去来回往返:

```bash
cargo fmt --all --check
cargo clippy --workspace --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::unwrap-used
cargo test --workspace --all-features --locked
```

需要真实数据库的集成测试(由 CI 的 integration job 执行):

```bash
cargo test -p server --test scenarios -- --ignored --test-threads=1
```

## 架构约定

- 每个业务 crate 采用六边形分层:`domain` / `application` / `ports` / `adapters`。**domain、application、ports 三层禁止引入任何 IO 依赖**(sqlx/axum/reqwest);各 crate 的 `tests/architecture.rs` 会强制校验——新建业务 crate 时请复制这个守卫。
- IO 适配器(pg/http)必须放在可选 feature 之后;只有组合根 `crates/server` 会显式启用它们。
- 新增 HTTP 端点:每个 handler 都必须携带 `AuthUser` 并做 `user.can(RESOURCE, ACTION)` 权限校验(公开鉴权端点除外);同时要在该文件的 `openapi()` 中注册,并保持 `security(("bearer" = []))` 注解。
- 数据库迁移放在 `crates/migrate/migrations/NNNN_name.sql`,版本号必须连续且不重复(重复会被守卫测试拦截);改完后执行 `touch crates/migrate/src/lib.rs` 再重新构建。
- Mutex 请用 `.lock().unwrap_or_else(std::sync::PoisonError::into_inner)`,不要用 `.expect("lock")`。

## 提交与 PR

- 提交信息采用约定式风格,小写单行:`feat(delivery): …` / `fix(web): …`。
- 一个 PR 只做一件事;要带测试;UI 改动请附截图。
- PR 描述应说明动机以及验证方式。
