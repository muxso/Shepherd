-- delivery 交付尝试补充创建时间(任务中心列表按时间排序/展示用)。
-- 历史行回填为当前时间(无更早的真实时间可考据)。
ALTER TABLE ms_delivery_attempt
    ADD COLUMN created_at BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint;

CREATE INDEX ix_ms_delivery_attempt_created ON ms_delivery_attempt (created_at DESC);
