//! PostgreSQL 适配器(feature = "pg")。实现 `UserRepository` 与 `UserDirectory`。
//! `PgUserDirectory.names_direct` 直查 ms_user 表,不经任何拦截 —— OIDC quirk 的修复本质。

use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use kernel::permission::PermissionSet;
use sqlx::{PgPool, Row};

use crate::domain::{
    Email, ExternalIdentity, NewOrganization, NewRole, NewUser, OidcError, Organization, Role,
    RoleScope, Session, User,
};
use crate::ports::{
    AuthRepoError, CredentialRepository, DirectoryError, ExternalUserRepository, LinkedUser,
    OrgRepoError, OrgRepository, RepoError, RoleRepoError, RoleRepository, SessionStore,
    UserCredential, UserDirectory, UserRepository, UserRoleRepository,
};

fn repo_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_user(row: &sqlx::postgres::PgRow) -> Result<User, RepoError> {
    let id: String = row.try_get("id").map_err(repo_err)?;
    let name: String = row.try_get("name").map_err(repo_err)?;
    let email_raw: String = row.try_get("email").map_err(repo_err)?;
    let email = Email::parse(&email_raw)
        .map_err(|_| RepoError::Backend(format!("corrupt email in db: {email_raw}")))?;
    Ok(User {
        id,
        name,
        email,
        enable: row.try_get("enable").map_err(repo_err)?,
        deleted: row.try_get("deleted").map_err(repo_err)?,
    })
}

#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_email(&self, email: &Email) -> Result<Option<User>, RepoError> {
        let row = sqlx::query(
            "SELECT id, name, email, enable, deleted FROM ms_user \
             WHERE email = $1 AND deleted = false",
        )
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(repo_err)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn insert(&self, new_user: &NewUser) -> Result<User, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_user (name, email) VALUES ($1, $2) RETURNING id, name, email, enable, deleted",
        )
        .bind(&new_user.name)
        .bind(new_user.email.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(repo_err)?;
        row_to_user(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<User>, RepoError> {
        let row = sqlx::query("SELECT id, name, email, enable, deleted FROM ms_user WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_err)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn count_active(&self) -> Result<u64, RepoError> {
        let row = sqlx::query("SELECT count(*) AS n FROM ms_user WHERE deleted = false")
            .fetch_one(&self.pool)
            .await
            .map_err(repo_err)?;
        Ok(row.try_get::<i64, _>("n").map_err(repo_err)?.max(0) as u64)
    }

    async fn list_active(&self, offset: u64, limit: u32) -> Result<Vec<User>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, email, enable, deleted FROM ms_user WHERE deleted = false \
             ORDER BY id LIMIT $1 OFFSET $2",
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(repo_err)?;
        rows.iter().map(row_to_user).collect()
    }

    async fn save(&self, user: &User) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_user SET name = $2, email = $3, enable = $4, deleted = $5 WHERE id = $1")
            .bind(&user.id)
            .bind(&user.name)
            .bind(user.email.as_str())
            .bind(user.enable)
            .bind(user.deleted)
            .execute(&self.pool)
            .await
            .map_err(repo_err)?;
        Ok(())
    }
}

/// 直查用户表的目录实现。重写里无 CFT 拦截器,故 `names_validated` 亦安全直查。
#[derive(Clone)]
pub struct PgUserDirectory {
    pool: PgPool,
}

impl PgUserDirectory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn query_names(&self, ids: &[String]) -> Result<BTreeMap<String, String>, DirectoryError> {
        let rows = sqlx::query("SELECT id, name FROM ms_user WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DirectoryError::Backend(e.to_string()))?;
        let mut map = BTreeMap::new();
        for r in &rows {
            let id: String = r.try_get("id").map_err(|e| DirectoryError::Backend(e.to_string()))?;
            let name: String =
                r.try_get("name").map_err(|e| DirectoryError::Backend(e.to_string()))?;
            map.insert(id, name);
        }
        Ok(map)
    }
}

#[async_trait]
impl UserDirectory for PgUserDirectory {
    async fn names_validated(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError> {
        self.query_names(ids).await
    }

    async fn names_direct(
        &self,
        ids: &[String],
    ) -> Result<BTreeMap<String, String>, DirectoryError> {
        self.query_names(ids).await
    }
}

fn auth_err(e: sqlx::Error) -> AuthRepoError {
    AuthRepoError::Backend(e.to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- 凭证仓储(ms_user_credential)----
#[derive(Clone)]
pub struct PgCredentialRepository {
    pool: PgPool,
}

impl PgCredentialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 幂等 upsert 一条凭证(供启动时种 admin)。
    pub async fn upsert(
        &self,
        username: &str,
        user_id: &str,
        password_hash: &str,
        permissions: &[String],
    ) -> Result<(), AuthRepoError> {
        sqlx::query(
            "INSERT INTO ms_user_credential (username, user_id, password_hash, permissions) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (username) DO UPDATE \
               SET user_id = EXCLUDED.user_id, password_hash = EXCLUDED.password_hash, \
                   permissions = EXCLUDED.permissions",
        )
        .bind(username)
        .bind(user_id)
        .bind(password_hash)
        .bind(permissions)
        .execute(&self.pool)
        .await
        .map_err(auth_err)?;
        Ok(())
    }
}

#[async_trait]
impl CredentialRepository for PgCredentialRepository {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError> {
        let row = sqlx::query(
            "SELECT user_id, password_hash, permissions FROM ms_user_credential WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(auth_err)?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some(UserCredential {
            user_id: r.try_get("user_id").map_err(auth_err)?,
            password_hash: r.try_get("password_hash").map_err(auth_err)?,
            permissions: r.try_get::<Vec<String>, _>("permissions").map_err(auth_err)?,
        }))
    }
}

// ---- 会话存储(ms_session)----
#[derive(Clone)]
pub struct PgSessionStore {
    pool: PgPool,
}

impl PgSessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionStore for PgSessionStore {
    async fn create(
        &self,
        user_id: &str,
        permissions: PermissionSet,
        ttl_secs: i64,
    ) -> Result<String, AuthRepoError> {
        let raw = permissions.to_raw();
        let now = now_ms();
        let row = sqlx::query(
            "INSERT INTO ms_session (user_id, permissions, created_at, expires_at)              VALUES ($1, $2, $3, $4) RETURNING token",
        )
        .bind(user_id)
        .bind(&raw)
        .bind(now)
        .bind(now + ttl_secs * 1000)
        .fetch_one(&self.pool)
        .await
        .map_err(auth_err)?;
        Ok(row.try_get::<String, _>("token").map_err(auth_err)?)
    }

    async fn get(&self, token: &str) -> Result<Option<Session>, AuthRepoError> {
        let row = sqlx::query(
            "SELECT user_id, permissions, expires_at FROM ms_session WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(auth_err)?;
        let Some(r) = row else { return Ok(None) };
        let expires_at_ms: i64 = r.try_get("expires_at").map_err(auth_err)?;
        if expires_at_ms <= now_ms() {
            return Ok(None); // 已过期
        }
        let user_id: String = r.try_get("user_id").map_err(auth_err)?;
        let raw: Vec<String> = r.try_get("permissions").map_err(auth_err)?;
        let permissions = PermissionSet::from_raw(&raw)
            .map_err(|_| AuthRepoError::Backend("corrupt session permissions".into()))?;
        Ok(Some(Session { token: token.to_string(), user_id, permissions, expires_at_ms }))
    }

    async fn revoke(&self, token: &str) -> Result<(), AuthRepoError> {
        sqlx::query("DELETE FROM ms_session WHERE token = $1")
            .bind(token)
            .execute(&self.pool)
            .await
            .map_err(auth_err)?;
        Ok(())
    }
}

// ---- 第三方身份 ↔ 本地用户映射(ms_external_user_link)----
#[derive(Clone)]
pub struct PgExternalUserRepository {
    pool: PgPool,
    default_perms: Vec<String>,
}

impl PgExternalUserRepository {
    pub fn new(pool: PgPool, default_perms: Vec<String>) -> Self {
        Self { pool, default_perms }
    }
}

#[async_trait]
impl ExternalUserRepository for PgExternalUserRepository {
    async fn find_or_provision(
        &self,
        identity: &ExternalIdentity,
    ) -> Result<LinkedUser, OidcError> {
        // 首次:按 default_perms 开通;已存在:只刷新 display_name,保留既有 user_id/permissions。
        let row = sqlx::query(
            "INSERT INTO ms_external_user_link (provider, open_id, display_name, permissions) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (provider, open_id) DO UPDATE SET display_name = EXCLUDED.display_name \
             RETURNING user_id, permissions",
        )
        .bind(&identity.provider)
        .bind(&identity.open_id)
        .bind(&identity.display_name)
        .bind(&self.default_perms)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OidcError::Backend(e.to_string()))?;
        Ok(LinkedUser {
            user_id: row.try_get("user_id").map_err(|e| OidcError::Backend(e.to_string()))?,
            permissions: row
                .try_get::<Vec<String>, _>("permissions")
                .map_err(|e| OidcError::Backend(e.to_string()))?,
        })
    }
}

fn org_err(e: sqlx::Error) -> OrgRepoError {
    OrgRepoError::Backend(e.to_string())
}

fn row_to_org(row: &sqlx::postgres::PgRow) -> Result<Organization, OrgRepoError> {
    Ok(Organization {
        id: row.try_get("id").map_err(org_err)?,
        name: row.try_get("name").map_err(org_err)?,
        enable: row.try_get("enable").map_err(org_err)?,
        deleted: row.try_get("deleted").map_err(org_err)?,
    })
}

// ---- 组织仓储(ms_organization)----
#[derive(Clone)]
pub struct PgOrgRepository {
    pool: PgPool,
}

impl PgOrgRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrgRepository for PgOrgRepository {
    async fn insert(&self, new_org: &NewOrganization) -> Result<Organization, OrgRepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_organization (name) VALUES ($1) RETURNING id, name, enable, deleted",
        )
        .bind(&new_org.name)
        .fetch_one(&self.pool)
        .await
        .map_err(org_err)?;
        row_to_org(&row)
    }

    async fn find_active_by_name(&self, name: &str) -> Result<Option<Organization>, OrgRepoError> {
        let row = sqlx::query(
            "SELECT id, name, enable, deleted FROM ms_organization WHERE name = $1 AND deleted = false",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(org_err)?;
        row.as_ref().map(row_to_org).transpose()
    }

    async fn get(&self, id: &str) -> Result<Option<Organization>, OrgRepoError> {
        let row = sqlx::query("SELECT id, name, enable, deleted FROM ms_organization WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(org_err)?;
        row.as_ref().map(row_to_org).transpose()
    }

    async fn count_active(&self) -> Result<u64, OrgRepoError> {
        let row = sqlx::query("SELECT count(*) AS n FROM ms_organization WHERE deleted = false")
            .fetch_one(&self.pool)
            .await
            .map_err(org_err)?;
        Ok(row.try_get::<i64, _>("n").map_err(org_err)?.max(0) as u64)
    }

    async fn list_active(&self, offset: u64, limit: u32) -> Result<Vec<Organization>, OrgRepoError> {
        let rows = sqlx::query(
            "SELECT id, name, enable, deleted FROM ms_organization WHERE deleted = false \
             ORDER BY seq LIMIT $1 OFFSET $2",
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(org_err)?;
        rows.iter().map(row_to_org).collect()
    }

    async fn save(&self, org: &Organization) -> Result<(), OrgRepoError> {
        sqlx::query("UPDATE ms_organization SET name = $2, enable = $3, deleted = $4 WHERE id = $1")
            .bind(&org.id)
            .bind(&org.name)
            .bind(org.enable)
            .bind(org.deleted)
            .execute(&self.pool)
            .await
            .map_err(org_err)?;
        Ok(())
    }
}

fn role_err(e: sqlx::Error) -> RoleRepoError {
    RoleRepoError::Backend(e.to_string())
}

fn row_to_role(row: &sqlx::postgres::PgRow) -> Result<Role, RoleRepoError> {
    let scope_raw: String = row.try_get("scope").map_err(role_err)?;
    let scope = RoleScope::parse(&scope_raw)
        .ok_or_else(|| RoleRepoError::Backend(format!("bad scope: {scope_raw}")))?;
    Ok(Role {
        id: row.try_get("id").map_err(role_err)?,
        name: row.try_get("name").map_err(role_err)?,
        scope,
        permissions: row.try_get::<Vec<String>, _>("permissions").map_err(role_err)?,
    })
}

const ROLE_COLS: &str = "id, name, scope, permissions";

#[derive(Clone)]
pub struct PgRoleRepository {
    pool: PgPool,
}
impl PgRoleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoleRepository for PgRoleRepository {
    async fn insert(&self, new_role: &NewRole) -> Result<Role, RoleRepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_role (name, scope, permissions) VALUES ($1, $2, $3) \
             RETURNING id, name, scope, permissions",
        )
        .bind(&new_role.name)
        .bind(new_role.scope.as_str())
        .bind(&new_role.permissions)
        .fetch_one(&self.pool)
        .await
        .map_err(role_err)?;
        row_to_role(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<Role>, RoleRepoError> {
        let row = sqlx::query(&format!("SELECT {ROLE_COLS} FROM ms_role WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(role_err)?;
        row.as_ref().map(row_to_role).transpose()
    }

    async fn count(&self) -> Result<u64, RoleRepoError> {
        let row = sqlx::query("SELECT count(*) AS n FROM ms_role")
            .fetch_one(&self.pool)
            .await
            .map_err(role_err)?;
        Ok(row.try_get::<i64, _>("n").map_err(role_err)?.max(0) as u64)
    }

    async fn list(&self, offset: u64, limit: u32) -> Result<Vec<Role>, RoleRepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {ROLE_COLS} FROM ms_role ORDER BY seq LIMIT $1 OFFSET $2"
        ))
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(role_err)?;
        rows.iter().map(row_to_role).collect()
    }

    async fn save(&self, role: &Role) -> Result<(), RoleRepoError> {
        sqlx::query("UPDATE ms_role SET name = $2, scope = $3, permissions = $4 WHERE id = $1")
            .bind(&role.id)
            .bind(&role.name)
            .bind(role.scope.as_str())
            .bind(&role.permissions)
            .execute(&self.pool)
            .await
            .map_err(role_err)?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), RoleRepoError> {
        sqlx::query("DELETE FROM ms_role WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(role_err)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct PgUserRoleRepository {
    pool: PgPool,
}
impl PgUserRoleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRoleRepository for PgUserRoleRepository {
    async fn grant(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError> {
        sqlx::query(
            "INSERT INTO ms_user_role (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(role_id)
        .execute(&self.pool)
        .await
        .map_err(role_err)?;
        Ok(())
    }

    async fn revoke(&self, user_id: &str, role_id: &str) -> Result<(), RoleRepoError> {
        sqlx::query("DELETE FROM ms_user_role WHERE user_id = $1 AND role_id = $2")
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await
            .map_err(role_err)?;
        Ok(())
    }

    async fn effective_permissions(&self, user_id: &str) -> Result<Vec<String>, RoleRepoError> {
        let rows = sqlx::query(
            "SELECT r.permissions AS perms FROM ms_role r \
             JOIN ms_user_role ur ON ur.role_id = r.id WHERE ur.user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(role_err)?;
        let mut set: HashSet<String> = HashSet::new();
        for row in &rows {
            set.extend(row.try_get::<Vec<String>, _>("perms").map_err(role_err)?);
        }
        Ok(set.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_org_crud_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_organization RESTART IDENTITY").execute(&pool).await.expect("trunc");

        let repo = PgOrgRepository::new(pool);
        let o = repo.insert(&NewOrganization::new("Acme").expect("v")).await.expect("insert");
        assert!(repo.find_active_by_name("Acme").await.expect("q").is_some());
        assert_eq!(repo.count_active().await.expect("c"), 1);

        let mut edited = repo.get(&o.id).await.expect("g").expect("some");
        edited.rename("AcmeCorp").expect("rename");
        edited.set_enabled(false);
        repo.save(&edited).await.expect("save");
        let got = repo.get(&o.id).await.expect("g").expect("some");
        assert_eq!(got.name, "AcmeCorp");
        assert!(!got.enable);

        // 软删除后名字释放、不计入 active
        let mut d = got;
        d.soft_delete();
        repo.save(&d).await.expect("save");
        assert_eq!(repo.count_active().await.expect("c"), 0);
        assert!(repo.find_active_by_name("AcmeCorp").await.expect("q").is_none());
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_auth_credential_and_session_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");

        let creds = PgCredentialRepository::new(pool.clone());
        creds
            .upsert("admin", "u-admin", "argon2-hash", &["SYSTEM_USER:READ+ADD".to_string()])
            .await
            .expect("upsert");
        // upsert 幂等:再来一次只更新
        creds
            .upsert("admin", "u-admin", "argon2-hash-2", &["SYSTEM_USER:READ".to_string()])
            .await
            .expect("upsert2");
        let c = creds.find_by_username("admin").await.expect("q").expect("some");
        assert_eq!(c.password_hash, "argon2-hash-2");
        assert_eq!(c.permissions, vec!["SYSTEM_USER:READ".to_string()]);
        assert!(creds.find_by_username("ghost").await.expect("q").is_none());

        // 会话:建 → 按令牌取回,权限往返
        let sessions = PgSessionStore::new(pool);
        let perms = PermissionSet::from_raw(["SYSTEM_USER:READ+ADD"]).expect("perm");
        let token = sessions.create("u-admin", perms, 3600).await.expect("create");
        let s = sessions.get(&token).await.expect("get").expect("some");
        assert_eq!(s.user_id, "u-admin");
        assert!(s.permissions.allows("SYSTEM_USER", "ADD"));
        assert!(sessions.get("no-such-token").await.expect("get").is_none());
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_user_roundtrip_and_directory() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");

        let repo = PgUserRepository::new(pool.clone());
        let nu = NewUser::new("Alice", "ss-probe@example.com").expect("valid");
        let existing = repo.find_by_email(&nu.email).await.expect("q");
        let user = match existing {
            Some(u) => u,
            None => repo.insert(&nu).await.expect("insert"),
        };

        let dir = PgUserDirectory::new(pool);
        let names = dir.names_direct(&[user.id.clone()]).await.expect("direct");
        assert_eq!(names.get(&user.id), Some(&"Alice".to_string()));
    }
}
