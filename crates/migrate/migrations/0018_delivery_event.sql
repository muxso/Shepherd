-- delivery 执行决策日志(审计轨迹):AI 执行者在一次交付尝试中逐步上报的决策/文件变更/测试结果等。
CREATE TABLE ms_delivery_event (
    seq         BIGSERIAL PRIMARY KEY,            -- 单调,审计排序用
    attempt_id  TEXT NOT NULL REFERENCES ms_delivery_attempt(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,                    -- DECISION / FILE_CHANGE / TEST_RESULT / TOOL_CALL / LOG
    message     TEXT NOT NULL,
    detail      TEXT,
    created_at  BIGINT NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint
);
CREATE INDEX ix_ms_delivery_event_attempt ON ms_delivery_event (attempt_id, seq);
