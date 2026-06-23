-- 接口定义存完整请求/响应规格(spec)。
-- 服务端只做不透明往返存取(不查询其内部),用 TEXT 存 JSON 文本即可,既保留 ApiDefinition 的 Eq 派生,
-- 又避免 JSONB ↔ String 的映射复杂度。结构(前端约定):
--   { "requestHeaders":[{name,value,desc}], "requestQuery":[...], "requestBody":"...", "responses":[{status,body}] }
ALTER TABLE ms_api_definition ADD COLUMN IF NOT EXISTS spec TEXT NOT NULL DEFAULT '{}';
