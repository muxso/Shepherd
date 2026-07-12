//! Database migrations: embeds the sqlx migrations under this crate's
//! migrations/ dir and runs them in order; also provides PgPool connect and
//! ping. After adding a migration, touch this file and rebuild or it won't be embedded.

use sqlx::migrate::MigrateError;
pub use sqlx::PgPool; // Re-export so the composition root can name PgPool without depending on sqlx directly.

pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(url).await
}

/// The migrator holds an advisory lock, so concurrent calls are safe.
/// Duplicate version numbers abort before anything runs: sqlx silently dedupes
/// them, which drops a migration (missing column -> 500 at runtime).
pub async fn run(pool: &PgPool) -> Result<(), MigrateError> {
    let migrator = sqlx::migrate!();
    assert_unique_versions(&migrator);
    migrator.run(pool).await
}

fn assert_unique_versions(migrator: &sqlx::migrate::Migrator) {
    let mut seen = std::collections::HashMap::new();
    for m in migrator.iter() {
        if let Some(other) = seen.insert(m.version, m.description.clone()) {
            panic!(
                "duplicate migration version {}: \"{}\" and \"{}\" — renumber one of them",
                m.version, other, m.description
            );
        }
    }
}

pub async fn ping(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}
