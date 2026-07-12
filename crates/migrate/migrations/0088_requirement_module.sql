-- 需求挂模块:复用共享的项目级 ms_module 树,空串 = 未规划(不设外键)。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS module_id TEXT NOT NULL DEFAULT '';
