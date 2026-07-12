//! Database migrations: embeds the sqlx migrations under this crate's
//! migrations/ dir and runs them in order; also provides PgPool connect and
//! ping. After adding a migration, touch this file and rebuild or it won't be embedded.

use sqlx::migrate::MigrateError;
pub use sqlx::PgPool; // Re-export so the composition root can name PgPool without depending on sqlx directly.

pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(url).await
}

/// The migrator holds an advisory lock, so concurrent calls are safe.
pub async fn run(pool: &PgPool) -> Result<(), MigrateError> {
    sqlx::migrate!().run(pool).await
}

pub async fn ping(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}
