use crate::{domain::*, ports::ProfileStore};
use async_trait::async_trait;
use plugin_runtime::PluginConfig;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseIdentity(String, Option<String>, Option<i32>);
impl DatabaseIdentity {
    pub async fn read(pool: &PgPool) -> Result<Self, StoreError> {
        let (name, address, port) = sqlx::query_as::<_, (String, Option<String>, Option<i32>)>(
            "SELECT current_database()::text, inet_server_addr()::text, inet_server_port()",
        )
        .fetch_one(pool)
        .await
        .map_err(db)?;
        Ok(Self(name, address, port))
    }
}

#[derive(Clone)]
pub struct PgProfileStore {
    pub(crate) pool: PgPool,
}
impl PgProfileStore {
    /// Compare actual database identity before running any DDL. The host supplies
    /// only its identity, never a business-data repository to a provider.
    pub async fn connect(url: &str, host: &DatabaseIdentity) -> Result<Self, StoreError> {
        let pool = PgPool::connect(url).await.map_err(db)?;
        if &DatabaseIdentity::read(&pool).await? == host {
            return Err(StoreError::HostDatabase);
        }
        sqlx::migrate!("./migrations").run(&pool).await.map_err(|_| StoreError::Unavailable)?;
        Ok(Self { pool })
    }
    async fn changed(&self, project: &str, id: &str) -> StoreError {
        match self.get(project, id).await {
            Ok(_) => StoreError::Conflict,
            Err(error) => error,
        }
    }
}
fn db(error: sqlx::Error) -> StoreError {
    match error.as_database_error().and_then(|e| e.code()).as_deref() {
        Some("23503" | "23505") => StoreError::Conflict,
        _ => StoreError::Unavailable,
    }
}
const COLS: &str = "id,project_id,plugin_id,name,config,enabled,revision";
fn profile(row: sqlx::postgres::PgRow) -> Result<Profile, StoreError> {
    let config: serde_json::Value = row.try_get("config").map_err(db)?;
    Ok(Profile {
        id: row.try_get("id").map_err(db)?,
        project_id: row.try_get("project_id").map_err(db)?,
        plugin_id: row.try_get("plugin_id").map_err(db)?,
        name: row.try_get("name").map_err(db)?,
        config: serde_json::from_value::<PluginConfig>(config)
            .map_err(|_| StoreError::Unavailable)?,
        enabled: row.try_get("enabled").map_err(db)?,
        revision: row.try_get("revision").map_err(db)?,
    })
}

#[async_trait]
impl ProfileStore for PgProfileStore {
    async fn create(&self, project: &str, new: NewProfile) -> Result<Profile, StoreError> {
        new.validate(project)?;
        let config = serde_json::to_value(&new.config).map_err(|_| StoreError::Invalid)?;
        let row=sqlx::query(&format!("INSERT INTO ep_profile(id,project_id,plugin_id,name,config,enabled) VALUES($1,$2,$3,$4,$5,$6) RETURNING {COLS}"))
            .bind(uuid::Uuid::new_v4().to_string()).bind(project).bind(new.plugin_id).bind(new.name.trim()).bind(config).bind(new.enabled)
            .fetch_one(&self.pool).await.map_err(db)?;
        profile(row)
    }
    async fn get(&self, project: &str, id: &str) -> Result<Profile, StoreError> {
        let row =
            sqlx::query(&format!("SELECT {COLS} FROM ep_profile WHERE project_id=$1 AND id=$2"))
                .bind(project)
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .ok_or(StoreError::NotFound)?;
        profile(row)
    }
    async fn list(&self, project: &str) -> Result<Vec<Profile>, StoreError> {
        sqlx::query(&format!("SELECT {COLS} FROM ep_profile WHERE project_id=$1 ORDER BY name,id"))
            .bind(project)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .into_iter()
            .map(profile)
            .collect()
    }
    async fn update(
        &self,
        project: &str,
        id: &str,
        revision: i64,
        new: NewProfile,
    ) -> Result<Profile, StoreError> {
        new.validate(project)?;
        if revision <= 0 {
            return Err(StoreError::Invalid);
        }
        let row=sqlx::query(&format!("UPDATE ep_profile SET name=$4,config=$5,enabled=$6,revision=revision+1,updated_at=now() WHERE project_id=$1 AND id=$2 AND revision=$3 AND plugin_id=$7 RETURNING {COLS}"))
            .bind(project).bind(id).bind(revision).bind(new.name.trim()).bind(serde_json::to_value(&new.config).map_err(|_| StoreError::Invalid)?)
            .bind(new.enabled).bind(new.plugin_id).fetch_optional(&self.pool).await.map_err(db)?;
        match row {
            Some(row) => profile(row),
            None => Err(self.changed(project, id).await),
        }
    }
    async fn delete(&self, project: &str, id: &str, revision: i64) -> Result<(), StoreError> {
        if revision <= 0 {
            return Err(StoreError::Invalid);
        }
        let deleted =
            sqlx::query("DELETE FROM ep_profile WHERE project_id=$1 AND id=$2 AND revision=$3")
                .bind(project)
                .bind(id)
                .bind(revision)
                .execute(&self.pool)
                .await
                .map_err(db)?
                .rows_affected();
        if deleted == 0 {
            return Err(self.changed(project, id).await);
        }
        Ok(())
    }
    async fn bind(&self, binding: Binding) -> Result<(), StoreError> {
        binding.validate()?;
        // Composite foreign key makes cross-project references impossible, even under races.
        self.get(&binding.project_id, &binding.profile_id).await?;
        sqlx::query("INSERT INTO ep_binding(project_id,environment_id,profile_id,slot,priority,enabled) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(project_id,environment_id,slot,profile_id) DO UPDATE SET priority=EXCLUDED.priority,enabled=EXCLUDED.enabled")
            .bind(binding.project_id).bind(binding.environment_id).bind(binding.profile_id).bind(binding.slot.as_str()).bind(binding.priority).bind(binding.enabled)
            .execute(&self.pool).await.map_err(db)?;
        Ok(())
    }
    async fn bindings(&self, project: &str, environment: &str) -> Result<Vec<Binding>, StoreError> {
        sqlx::query("SELECT project_id,environment_id,profile_id,slot,priority,enabled FROM ep_binding WHERE project_id=$1 AND environment_id=$2 ORDER BY priority,slot,profile_id")
            .bind(project).bind(environment).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| {
                let slot: String=r.try_get("slot").map_err(db)?;
                Ok(Binding { project_id:r.try_get("project_id").map_err(db)?, environment_id:r.try_get("environment_id").map_err(db)?,
                    profile_id:r.try_get("profile_id").map_err(db)?, slot:serde_json::from_value(serde_json::Value::String(slot)).map_err(|_| StoreError::Unavailable)?,
                    priority:r.try_get("priority").map_err(db)?, enabled:r.try_get("enabled").map_err(db)? })
            }).collect()
    }
    async fn unbind(
        &self,
        project: &str,
        environment: &str,
        slot: BindingSlot,
        profile: &str,
    ) -> Result<(), StoreError> {
        let deleted=sqlx::query("DELETE FROM ep_binding WHERE project_id=$1 AND environment_id=$2 AND slot=$3 AND profile_id=$4")
            .bind(project).bind(environment).bind(slot.as_str()).bind(profile).execute(&self.pool).await.map_err(db)?.rows_affected();
        if deleted == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
