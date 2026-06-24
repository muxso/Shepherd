-- 用例结果实际请求:执行器记录每步**实际发送**的请求(method/url/请求头/请求体,
-- 变量 ${...} / 环境 baseUrl / 认证头均已解析)。供场景报告「实际请求/控制台/cURL」
-- 100% 还原已发请求(此前仅前端据用例模板近似重建,带变量则不准)。旧报告/未回填为 NULL。
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS req_method  TEXT;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS req_url     TEXT;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS req_headers JSONB;
ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS req_body    TEXT;
