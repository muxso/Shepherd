# Shepherd Yunxiao synchronization plugin

Yunxiao facts live in a separate plugin database. The plugin shares Shepherd authentication and its UI entry point, but never writes native projects, requirements, cases, directories, tasks, deliveries, or statistics. Native statistics do not join plugin tables.

## Installation

Build the standard Shepherd server and web artifacts. The plugin is included by default; `cargo build -p server --no-default-features` excludes it.

Create a separate PostgreSQL database and a dedicated account with permissions limited to that database. Configure `SHEPHERD_PLUGINS=yunxiao` and `SHEPHERD_YUNXIAO_DATABASE_URL=postgres://.../shepherd_yunxiao`, then restart. Enabling the plugin without its database configuration, or pointing it at the Shepherd database, fails startup. There is no fallback to the host database.

Plugin migrations live under `plugins/yunxiao/backend/migrations/`. The `ms_devops_*` tables exist only in the plugin database and have no native business foreign keys. The legacy `projectId` field is an opaque reference, not a native project lookup.

Open System → Plugins to inspect status and enter the Yunxiao dashboard. Disabling the plugin unmounts its API and hides its menu without opening or migrating its database.

## Collection

The read-only collector and tests live under `plugins/yunxiao/scripts/`. It reuses the shared collector configured through `SHEPHERD_COLLECTOR_PATH` and requires the bundled `yunxiao-collector.patch` extension. It sends no messages and performs no writes to Yunxiao.

Store private configuration in `~/.config/shepherd-yunxiao/env` with mode 600. Configure `SHEPHERD_URL`, `SHEPHERD_SYNC_KEY`, `SHEPHERD_SOURCE_ID`, `SHEPHERD_COLLECTOR_PATH`, `SHEPHERD_SYNC_DAYS`, and `SHEPHERD_TEST_REPO_IDS`. Run:

```sh
python3 plugins/yunxiao/scripts/yunxiao_sync.py ~/.config/shepherd-yunxiao/env
```

User timer units live under `plugins/yunxiao/deploy/` and assume checkout path `~/Shepherd`; adjust the paths when needed. Private detail caches and replay snapshots remain under `~/.cache/shepherd-yunxiao/`. A complete, validated snapshot is published transactionally in the plugin database; failed collection preserves the previous successful snapshot.

## Existing installations

Stop the collector timer when disabling the plugin. Existing host migrations 0112/0113 retain their original checksums for validation, but fresh host databases skip them. Existing tables, source columns, and projected native records are not deleted automatically. Back up and inspect any manually edited records before a separate cleanup. Copy existing external facts to the independent plugin database or recollect them before enabling the new collector.

The October 28 validation documents describe the superseded integration and are historical records only.

## Validation

```sh
cargo test -p shepherd-yunxiao --all-features --locked
cargo test -p migrate --locked
cargo test -p server --bin server plugins::tests --locked
cargo check -p server --no-default-features --locked
python3 -m unittest discover -s plugins/yunxiao/scripts -p 'test_*.py' -v
```

Run database integration tests only with a disposable `SHEPHERD_YUNXIAO_TEST_DATABASE_URL`: `cargo test -p shepherd-yunxiao --all-features --test pg_sync -- --ignored`. They verify synchronization without native business tables.

The [Chinese guide](YUNXIAO.zh-CN.md) includes the same operational boundaries.
