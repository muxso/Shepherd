# 数据迁移:MySQL → PostgreSQL

把 MeterSphere 现网 MySQL 数据搬到 Rust 重写所用的 PostgreSQL。schema 由
`migrate`(版本化迁移)创建,本目录只负责**数据搬运 + 切换流程**。

> ⚠️ **坑(已踩)**:新增 `crates/migrate/migrations/*.sql` 后,`sqlx::migrate!` 宏的变更检测
> 可能不触发依赖方重编译 → 二进制里嵌的是**旧迁移集**,运行时报 `relation ... does not exist`。
> 解决:`touch crates/migrate/src/lib.rs && cargo build`(或 `cargo clean -p migrate`)强制重嵌,
> 且**重建所有用到迁移的二进制**(尤其 `server`),再启动。

## 组成

- `mysql-to-pg.load` —— pgloader 命令文件(`WITH data only`,用 MySQL 侧
  MATERIALIZE VIEWS 把源表重塑成我们裁剪过的 `ms_*` 目标列)。
- `run.sh` —— 驱动脚本:① 建 PG schema → ② pgloader 迁数据 → ③ 行数核对提示。

## schema 真源

PG 表结构的**唯一真源**是 `crates/ms-migrate/migrations/*.sql`(版本化)。
应用迁移有两种方式(幂等,记录在 `_sqlx_migrations`):

```bash
# 只建表/演进 schema 然后退出
DATABASE_URL=postgres://msuser:mspass@host:5432/mstest \
  cargo run -p ms-server -- --migrate-only

# 或:正常启动服务时自动 migrate
DATABASE_URL=... cargo run -p ms-server
```

## 跑迁移

```bash
brew install pgloader   # 或 apt-get install pgloader
export MYSQL_URL="mysql://user:pass@mysql-host:3306/metersphere"
export PG_URL="postgres://msuser:mspass@pg-host:5432/mstest"

DRY_RUN=1 ./migration/run.sh   # 先看将执行什么,不动数据
./migration/run.sh             # 正式迁移
```

## 必须正视的点

1. **`mysql-to-pg.load` 里的视图 SELECT 是模板**,源列名按 MeterSphere 习惯写,
   务必对照真实 MySQL schema 校正(组织/项目外键列、软删除列、tinyint 布尔)。
2. **保留源端既有 id**:带 `gen_random_uuid()` 默认值的 id 列在迁移时应沿用源 id,
   否则跨表外键(case_id / project_id / pool_id)会对不上。
3. **先 dry-run、再在副本上全量演练**,核对关键表行数与抽样数据。
4. 换引擎后**无法与 Java 共用活库**:策略是一次性迁移 + 短暂影子双跑对比响应 + 一次性切换,
   而非长期 MySQL↔PG 双写。
5. 目前 `ms_*` 是裁剪过的子集(只含 Rust 侧用到的列);未迁移的历史列按需在迁移文件中补列再迁。
