//! 报告归档后台扫描:周期性把已完成(SUCCESS/ERROR)且未归档的报告导出为 Parquet 冷存储。
//!
//! 与热路径解耦——只读 PG 报告读模型,写到 object_store(本地目录/S3)。开关:
//!   `REPORT_ARCHIVE_DIR`          —— 归档根目录(未设则不启用,直接返回)。
//!   `REPORT_ARCHIVE_INTERVAL_SECS`—— 扫描间隔秒(默认 60)。
//!   `REPORT_ARCHIVE_BATCH`        —— 单轮最多归档数(默认 100)。
//! 查询用 DuckDB 直接扫目录,见 [`api_test::adapters::report_archive`]。

use std::time::Duration;

use api_test::adapters::pg::PgBatchReport;
use api_test::adapters::ReportArchiver;
use sqlx::PgPool;

/// 若配置了 `REPORT_ARCHIVE_DIR` 则 spawn 后台扫描任务;否则记一行 info 后返回。
pub fn spawn(pool: PgPool) {
    let Ok(dir) = std::env::var("REPORT_ARCHIVE_DIR") else {
        tracing::info!("report archive disabled (set REPORT_ARCHIVE_DIR to enable)");
        return;
    };
    let dir = dir.trim().to_string();
    if dir.is_empty() {
        tracing::info!("report archive disabled (empty REPORT_ARCHIVE_DIR)");
        return;
    }
    let interval = env_u64("REPORT_ARCHIVE_INTERVAL_SECS", 60).max(5);
    let batch = env_u64("REPORT_ARCHIVE_BATCH", 100).clamp(1, 1000) as i64;

    // 目录需先存在(LocalFileSystem 要求)。建不出来就放弃启用,不影响主流程。
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(%dir, error = %e, "report archive: cannot create dir, disabled");
        return;
    }
    let archiver = match ReportArchiver::new_local(&dir, "reports") {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(%dir, error = %e, "report archive: cannot open object store, disabled");
            return;
        }
    };
    let reports = PgBatchReport::new(pool);
    tracing::info!(%dir, interval_secs = interval, batch, "report archive enabled");

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(interval));
        loop {
            tick.tick().await;
            match reports.list_unarchived(batch).await {
                Ok(ids) if !ids.is_empty() => {
                    let mut ok = 0usize;
                    for id in &ids {
                        match reports.detail(id).await {
                            Ok(Some(d)) => match archiver.archive(id, &d).await {
                                Ok(_) => {
                                    // 归档成功才打标;失败留待下轮重试(幂等:同键覆盖)。
                                    if let Err(e) = reports.mark_archived(id).await {
                                        tracing::warn!(report = %id, error = %e, "report archive: mark failed");
                                    } else {
                                        ok += 1;
                                    }
                                }
                                Err(e) => tracing::warn!(report = %id, error = %e, "report archive: write failed"),
                            },
                            // 报告无明细(理论不该出现):直接打标,避免反复扫描。
                            Ok(None) => {
                                let _ = reports.mark_archived(id).await;
                            }
                            Err(e) => tracing::warn!(report = %id, error = %e, "report archive: read failed"),
                        }
                    }
                    if ok > 0 {
                        tracing::info!(archived = ok, total = ids.len(), "report archive: batch done");
                    }
                }
                Ok(_) => {} // 无待归档
                Err(e) => tracing::warn!(error = %e, "report archive: scan failed"),
            }
        }
    });
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}
