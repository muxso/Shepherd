-- 定向派发:把交付尝试指定给某个注册 runtime(按 name,跨重连稳定)。
-- NULL = 未定向,该能力的任意 runtime 都可认领。
ALTER TABLE ms_delivery_attempt ADD COLUMN IF NOT EXISTS target_runtime TEXT;
