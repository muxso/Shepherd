-- 资源池主键补默认值:与 ms_environment 一致用 gen_random_uuid()::text。
-- 原表 id 无默认,创建只能由调用方手填 id;补上后服务端 INSERT 可省略 id 自动生成。
ALTER TABLE ms_resource_pool ALTER COLUMN id SET DEFAULT gen_random_uuid()::text;
