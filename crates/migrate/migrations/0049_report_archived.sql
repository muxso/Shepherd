-- 0049: 报告归档标记(Parquet 冷存储)。已完成报告导出为 Parquet 后写 archived_at,
-- 归档扫描据此跳过已处理报告。部分索引只覆盖「未归档」,扫描廉价。
ALTER TABLE ms_api_batch_report ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS ix_api_batch_report_unarchived
    ON ms_api_batch_report (id) WHERE archived_at IS NULL;
