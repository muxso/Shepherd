-- 报告头加起止时间,用于「报告总耗时」(wall-clock)。
-- started_at 在 create 时写 now();finished_at 在 set_status(终态)时写 now()。旧报告两者为 NULL。
ALTER TABLE ms_api_batch_report ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ;
ALTER TABLE ms_api_batch_report ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ;
