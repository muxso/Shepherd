-- 回填历史:0079 加列前已验收的任务没有 verified_at。
-- AI 交付的任务用其最近一次 DELIVERED 交付记录的时间近似(验收紧随交付,误差可接受);
-- 无交付记录的人工任务无从考证,保持 NULL(贡献格子自然缺席)。幂等:只填 NULL。
UPDATE ms_task t
SET verified_at = (
    -- created_at 是毫秒 BIGINT(库内约定),转 timestamptz
    SELECT to_timestamp(max(a.created_at) / 1000.0) FROM ms_delivery_attempt a
    WHERE a.decomposition_id = t.decomposition_id AND a.task_id = t.id AND a.status = 'DELIVERED'
)
WHERE t.status = 'VERIFIED' AND t.verified_at IS NULL
  AND EXISTS (
    SELECT 1 FROM ms_delivery_attempt a
    WHERE a.decomposition_id = t.decomposition_id AND a.task_id = t.id AND a.status = 'DELIVERED'
  );
