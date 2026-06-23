-- 0048: 持久化逐条断言结果 + 提取变量(报告展开用;通过项也保留)。
-- assertions:  [{item,condition,expected,actual,passed,reason}, …]
-- extractions: [[变量名, 值], …]
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS assertions  JSONB;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS extractions JSONB;
