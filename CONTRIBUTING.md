# Contributing

> 简体中文版见 [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md).

Issues and PRs are welcome. Here is the shortest path to a mergeable PR.

## Local setup

```bash
# Rust toolchain pins to CI (.github/workflows/ci.yml RUST_TOOLCHAIN)
rustup toolchain install 1.86.0 --component clippy rustfmt

# Backend: needs PostgreSQL (integration tests and local run)
docker run -d --name shepherd-pg -e POSTGRES_USER=msuser -e POSTGRES_PASSWORD=mspass \
  -e POSTGRES_DB=mstest -p 55432:5432 postgres:16-alpine
cargo run                      # connects to localhost:55432 by default, migrations auto-run

# Frontend
cd web && npm ci && npm run dev   # Vite proxies to 127.0.0.1:9180
```

## Pre-flight checks

CI runs these four; run them locally first to save a round-trip:

```bash
cargo fmt --all --check
cargo clippy --workspace --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::unwrap-used
cargo test --workspace --all-features --locked
```

Integration tests that need a real database (run by CI's integration job):

```bash
cargo test -p server --test scenarios -- --ignored --test-threads=1
```

## Architecture conventions

- Every business crate uses hexagonal layering: `domain` / `application` / `ports` / `adapters`. **The domain, application, and ports layers forbid any IO dependency** (sqlx/axum/reqwest); each crate's `tests/architecture.rs` enforces this — copy the guard into new business crates.
- IO adapters (pg/http) must sit behind optional features; only `crates/server` (the composition root) enables them explicitly.
- New HTTP endpoints: every handler must carry `AuthUser` and perform a `user.can(RESOURCE, ACTION)` permission check (except public auth endpoints); also register it in that file's `openapi()` and keep the `security(("bearer" = []))` annotation.
- Database migrations go in `crates/migrate/migrations/NNNN_name.sql`, with consecutive, non-duplicate version numbers (duplicates are caught by the guard test); after editing, run `touch crates/migrate/src/lib.rs` then rebuild.
- Use `.lock().unwrap_or_else(std::sync::PoisonError::into_inner)` for Mutexes, not `.expect("lock")`.

## Commits and PRs

- Commit messages use conventional style, lowercase single line: `feat(delivery): …` / `fix(web): …`.
- One PR does one thing; include tests; UI changes attach screenshots.
- PR description should state the motivation and how it was verified.
