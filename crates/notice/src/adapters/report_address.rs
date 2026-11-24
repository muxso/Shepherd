use sqlx::PgPool;
use url::Url;

/// Report links are rooted at the Shepherd origin.
pub fn normalize(value: &str) -> Result<String, &'static str> {
    let text = value.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    let (_, address) = text.split_once("://").ok_or("Enter an HTTP or HTTPS origin")?;
    let url = Url::parse(text).map_err(|_| "Enter an HTTP or HTTPS origin")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || address.starts_with('/')
        || address.contains(char::is_whitespace)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Enter an HTTP or HTTPS origin without a path, credentials, query or fragment");
    }
    Ok(url.origin().ascii_serialization())
}

pub fn deployment_default() -> Option<String> {
    let raw = std::env::var("SHEPHERD_PUBLIC_URL").ok()?;
    normalize(&raw).ok().filter(|origin| !origin.is_empty())
}

pub async fn configured(pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT base_url FROM ms_report_address WHERE id = 1")
        .fetch_optional(pool)
        .await
}

pub async fn effective(pool: &PgPool) -> Option<String> {
    match configured(pool).await {
        Ok(value) => value.or_else(deployment_default),
        Err(error) => {
            tracing::warn!(%error, "unable to load configured report address");
            None
        }
    }
}

pub async fn save(pool: &PgPool, origin: &str) -> Result<(), sqlx::Error> {
    if origin.is_empty() {
        sqlx::query("DELETE FROM ms_report_address WHERE id = 1").execute(pool).await?;
    } else {
        sqlx::query(
            "INSERT INTO ms_report_address (id, base_url) VALUES (1, $1)
             ON CONFLICT (id) DO UPDATE SET base_url = EXCLUDED.base_url",
        )
        .bind(origin)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_origins_and_rejects_paths_and_sensitive_components() {
        assert_eq!(
            normalize(" https://Reports.Example.org:443/ "),
            Ok("https://reports.example.org".into())
        );
        assert_eq!(normalize("http://192.0.2.10:8080"), Ok("http://192.0.2.10:8080".into()));
        assert_eq!(normalize(" "), Ok(String::new()));
        for bad in [
            "example.org",
            "ftp://example.org",
            "https://user:secret@example.org",
            "https://example.org/report",
            "https://example.org/?key=1",
            "https://example.org/#section",
            "https:///example.org",
        ] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
    }
}
