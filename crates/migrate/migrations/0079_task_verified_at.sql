-- 任务首次验收通过的时间:人机协同人效的周趋势时间轴。
-- NULL = 尚未通过或历史数据(加列前已通过的任务不回填,趋势图自然缺席)。
ALTER TABLE ms_task ADD COLUMN IF NOT EXISTS verified_at TIMESTAMPTZ;
