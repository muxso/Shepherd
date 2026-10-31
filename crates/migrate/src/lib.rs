//! Legacy Yunxiao migrations are validation-only; new host databases never apply them.
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
    // Keep historical checksums for already-deployed databases, without installing
    // plugin tables or source columns in a fresh Shepherd database.
    let has_history: bool =
        sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    let applied: Vec<i64> = if has_history {
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version IN (112,113)")
            .fetch_all(pool)
            .await?
    } else {
        Vec::new()
    };
    host_migrations(&applied).run(pool).await
}

fn host_migrations(applied_legacy: &[i64]) -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!();
    assert_unique_versions(&migrator);
    migrator
        .migrations
        .to_mut()
        .retain(|m| !matches!(m.version, 112 | 113) || applied_legacy.contains(&m.version));
    migrator
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_hosts_never_install_legacy_plugin_migrations() {
        let host = host_migrations(&[]);
        assert!(host.iter().all(|m| !matches!(m.version, 112 | 113)));
        assert!(host.iter().any(|m| m.version == 111));
    }

    #[test]
    fn deployed_hosts_keep_exact_checksums_without_applying_pending_legacy_migrations() {
        for applied in [&[112][..], &[112, 113][..]] {
            let host = host_migrations(applied);
            let original = sqlx::migrate!();
            for version in [112, 113] {
                let kept = host.iter().find(|m| m.version == version);
                assert_eq!(kept.is_some(), applied.contains(&version));
                if let Some(kept) = kept {
                    assert_eq!(
                        kept.checksum,
                        original
                            .iter()
                            .find(|m| m.version == version)
                            .expect("legacy migration")
                            .checksum
                    );
                }
            }
        }
    }
}
