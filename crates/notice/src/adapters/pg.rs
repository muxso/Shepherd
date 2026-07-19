// `created_at` is surfaced as epoch millis so the web can render relative times
// without parsing PG's timestamptz text format.

use async_trait::async_trait;

use crate::domain::{NewNotice, Notice};
use crate::ports::{
    ListQuery, NoticePage, NoticeStore, NoticeUserDirectory, RepoError, Tab, UnreadCount,
};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgNoticeStore {
    pool: PgPool,
}

impl PgNoticeStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const COLS: &str = "id::text AS id, project_id, receiver_id, category, event_type, title, \
     content, resource_type, resource_id, operator, at_mention, read, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_at";

// Shared receiver/project/category/tab predicate ($1..$4); unscoped notices
// (project_id = '') match every project.
const FILTER: &str = "receiver_id = $1 \
     AND ($2::text IS NULL OR project_id = '' OR project_id = $2) \
     AND ($3::text IS NULL OR category = $3) \
     AND ($4::text <> 'at' OR at_mention) \
     AND ($4::text <> 'unread' OR NOT read) \
     AND ($4::text <> 'read' OR read)";

fn tab_str(tab: Tab) -> &'static str {
    match tab {
        Tab::All => "all",
        Tab::At => "at",
        Tab::Unread => "unread",
        Tab::Read => "read",
    }
}

fn row_to_notice(row: &sqlx::postgres::PgRow) -> Result<Notice, RepoError> {
    Ok(Notice {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        receiver_id: row.try_get("receiver_id").map_err(map_err)?,
        category: row.try_get("category").map_err(map_err)?,
        event_type: row.try_get("event_type").map_err(map_err)?,
        title: row.try_get("title").map_err(map_err)?,
        content: row.try_get("content").map_err(map_err)?,
        resource_type: row.try_get("resource_type").map_err(map_err)?,
        resource_id: row.try_get("resource_id").map_err(map_err)?,
        operator: row.try_get("operator").map_err(map_err)?,
        at_mention: row.try_get("at_mention").map_err(map_err)?,
        read: row.try_get("read").map_err(map_err)?,
        created_at: row.try_get("created_at").map_err(map_err)?,
    })
}

#[async_trait]
impl NoticeStore for PgNoticeStore {
    async fn insert(&self, notice: &NewNotice) -> Result<usize, RepoError> {
        // One statement per fan-out target; receiver lists are small (mentions/members).
        let mut written = 0;
        for receiver in &notice.receivers {
            sqlx::query(
                "INSERT INTO ms_notice (project_id, receiver_id, category, event_type, title, \
                 content, resource_type, resource_id, operator, at_mention) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(&notice.project_id)
            .bind(receiver)
            .bind(&notice.category)
            .bind(&notice.event_type)
            .bind(&notice.title)
            .bind(&notice.content)
            .bind(&notice.resource_type)
            .bind(&notice.resource_id)
            .bind(&notice.operator)
            .bind(notice.at_mention)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
            written += 1;
        }
        Ok(written)
    }

    async fn list(&self, query: &ListQuery) -> Result<NoticePage, RepoError> {
        let total: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM ms_notice WHERE {FILTER}"))
                .bind(&query.receiver_id)
                .bind(query.project_id.as_deref())
                .bind(query.category.as_deref())
                .bind(tab_str(query.tab))
                .fetch_one(&self.pool)
                .await
                .map_err(map_err)?;

        let offset = i64::from(query.page.max(1) - 1) * i64::from(query.page_size);
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_notice WHERE {FILTER} \
             ORDER BY created_at DESC, id LIMIT $5 OFFSET $6"
        ))
        .bind(&query.receiver_id)
        .bind(query.project_id.as_deref())
        .bind(query.category.as_deref())
        .bind(tab_str(query.tab))
        .bind(i64::from(query.page_size))
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let items = rows.iter().map(row_to_notice).collect::<Result<Vec<_>, _>>()?;
        Ok(NoticePage { items, total: total.max(0) as u64 })
    }

    async fn unread_count(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<UnreadCount, RepoError> {
        let rows = sqlx::query(
            "SELECT category, count(*) AS n FROM ms_notice \
             WHERE receiver_id = $1 AND NOT read \
             AND ($2::text IS NULL OR project_id = '' OR project_id = $2) \
             GROUP BY category",
        )
        .bind(receiver_id)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut out = UnreadCount::default();
        for row in rows {
            let category: String = row.try_get("category").map_err(map_err)?;
            let n: i64 = row.try_get("n").map_err(map_err)?;
            out.total += n.max(0) as u64;
            out.by_category.push((category, n.max(0) as u64));
        }
        Ok(out)
    }

    async fn mark_read(&self, id: &str, receiver_id: &str) -> Result<bool, RepoError> {
        // id::text comparison keeps malformed ids a plain miss instead of a cast error.
        let res = sqlx::query(
            "UPDATE ms_notice SET read = true WHERE id::text = $1 AND receiver_id = $2",
        )
        .bind(id)
        .bind(receiver_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn mark_all_read(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<u64, RepoError> {
        let res = sqlx::query(
            "UPDATE ms_notice SET read = true WHERE receiver_id = $1 AND NOT read \
             AND ($2::text IS NULL OR project_id = '' OR project_id = $2)",
        )
        .bind(receiver_id)
        .bind(project_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected())
    }
}

/// Resolves receivers against ms_user_credential (login accounts) and ms_user
/// (directory/OIDC users); project membership comes from ms_project_member.
#[derive(Clone)]
pub struct PgNoticeUserDirectory {
    pool: PgPool,
}

impl PgNoticeUserDirectory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NoticeUserDirectory for PgNoticeUserDirectory {
    async fn resolve_user_ids(&self, names: &[String]) -> Result<Vec<String>, RepoError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT user_id FROM ms_user_credential \
             WHERE username = ANY($1) OR user_id = ANY($1) \
             UNION SELECT id FROM ms_user WHERE name = ANY($1) OR id = ANY($1)",
        )
        .bind(names)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(rows)
    }

    async fn project_member_ids(&self, project_id: &str) -> Result<Vec<String>, RepoError> {
        sqlx::query_scalar::<_, String>(
            "SELECT user_id FROM ms_project_member WHERE project_id = $1",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_notice_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_notice").execute(&pool).await.expect("truncate");

        let store = PgNoticeStore::new(pool.clone());
        let n = NewNotice {
            project_id: "p1".into(),
            receivers: vec!["u1".into(), "u2".into()],
            category: "BUG".into(),
            event_type: "BUG_ASSIGNED".into(),
            title: "登录页崩溃".into(),
            content: String::new(),
            resource_type: "BUG".into(),
            resource_id: "b1".into(),
            operator: "admin".into(),
            at_mention: false,
        };
        assert_eq!(store.insert(&n).await.expect("insert"), 2);

        let page = store
            .list(&ListQuery {
                receiver_id: "u1".into(),
                project_id: Some("p1".into()),
                page: 1,
                page_size: 10,
                ..Default::default()
            })
            .await
            .expect("list");
        assert_eq!(page.total, 1);
        assert!(page.items[0].created_at > 0);

        let unread = store.unread_count("u1", Some("p1")).await.expect("count");
        assert_eq!(unread.total, 1);
        assert!(store.mark_read(&page.items[0].id, "u1").await.expect("read"));
        assert_eq!(store.unread_count("u1", Some("p1")).await.expect("count").total, 0);
        assert_eq!(store.mark_all_read("u2", None).await.expect("all"), 1);
    }
}
