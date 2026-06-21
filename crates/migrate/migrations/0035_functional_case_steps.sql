-- 功能用例:手工测试步骤(步骤描述 + 预期结果),jsonb 数组 [{step, expected}]。
ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS steps jsonb NOT NULL DEFAULT '[]'::jsonb;
