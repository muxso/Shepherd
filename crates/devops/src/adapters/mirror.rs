//! Transactional projections into native project lists. Never creates task or delivery events.
use super::pg::db;
use crate::domain::Error;
pub async fn project(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source: &str,
) -> Result<(), Error> {
    let project = sqlx::query_scalar::<_, String>(
        "SELECT project_id FROM ms_devops_source WHERE id=$1 FOR UPDATE",
    )
    .bind(source)
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    // Serialize display number allocation between source projections in the same project.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&project)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    for sql in [
        include_str!("mirror_modules.sql"),
        include_str!("mirror_requirements.sql"),
        include_str!("mirror_cases.sql"),
    ] {
        sqlx::query(sql).bind(source).bind(&project).execute(&mut **tx).await.map_err(db)?;
    }
    for sql in [include_str!("mirror_versions.sql"), include_str!("mirror_stages.sql")] {
        sqlx::query(sql).bind(source).execute(&mut **tx).await.map_err(db)?;
    }
    sqlx::query("UPDATE ms_requirement SET deleted=true WHERE external_source_id=$1 AND NOT deleted AND NOT EXISTS(SELECT 1 FROM ms_devops_entity e WHERE e.source_id=$1 AND e.kind='requirement' AND e.external_id=ms_requirement.external_id AND e.active)").bind(source).execute(&mut **tx).await.map_err(db)?;
    sqlx::query("UPDATE ms_functional_case SET deleted=true WHERE external_source_id=$1 AND NOT deleted AND NOT EXISTS(SELECT 1 FROM ms_devops_entity e WHERE e.source_id=$1 AND e.kind='testcase' AND e.external_id=ms_functional_case.external_id AND e.active)").bind(source).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
