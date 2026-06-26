use sqlx::migrate::MigrateError;
pub use sqlx::PgPool; // 重导出:组装根可命名 PgPool 而不直接依赖 sqlx

pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(url).await
}

/// 迁移器持有 advisory lock,可并发安全调用。
pub async fn run(pool: &PgPool) -> Result<(), MigrateError> {
    sqlx::migrate!().run(pool).await
}

pub async fn ping(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}
