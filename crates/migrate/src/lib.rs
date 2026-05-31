//! 数据库迁移器。`migrations/` 下的版本化 SQL 在编译期被嵌入二进制,
//! 运行期由 sqlx 迁移器按序施加,并在 `_sqlx_migrations` 表记录已应用版本(幂等)。
//!
//! 生产与集成测试都调用 [`run`],确保 schema 单一真源、可重复。

use sqlx::migrate::MigrateError;
pub use sqlx::PgPool; // 重导出,组装根可命名 PgPool(如 /readyz 的 State)而不直接依赖 sqlx

/// 连接到 PG(不建表)。
pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(url).await
}

/// 施加所有尚未应用的迁移(幂等;迁移器持有 advisory lock,可并发安全调用)。
pub async fn run(pool: &PgPool) -> Result<(), MigrateError> {
    sqlx::migrate!().run(pool).await
}

/// 就绪探针:能否对 PG 跑通一次轻量查询。
pub async fn ping(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}
