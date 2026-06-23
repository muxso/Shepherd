-- 资源池补全字段(对齐参考 UI:列表 + Node/Kubernetes 新增表单)。
-- 原表仅 id/name/enabled/deleted,无法承载 描述、最大并发、类型、应用组织、节点/K8s 配置、时间。
-- 这里补齐为可读可写的管理面字段;类型相关的节点配置落 JSONB(config),组织范围落 JSONB(org_ids)。
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS description     TEXT        NOT NULL DEFAULT '';
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS max_concurrency INTEGER     NOT NULL DEFAULT 0;
-- 'Node' | 'Kubernetes'
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS pool_type       TEXT        NOT NULL DEFAULT 'Node';
-- 应用组织:true=全部组织;false=仅 org_ids 指定的组织
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS all_org         BOOLEAN     NOT NULL DEFAULT true;
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS org_ids         JSONB       NOT NULL DEFAULT '[]'::jsonb;
-- 工作节点 URL(MS 部署地址)
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS server_url      TEXT        NOT NULL DEFAULT '';
-- 类型相关配置:Node 为 {nodes:[{ip,port,concurrentNumber,singleTaskConcurrentNumber}]};
-- Kubernetes 为 {ip,token,namespace,deployName,concurrentNumber,singleTaskConcurrentNumber}
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS config          JSONB       NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS created_at      TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE ms_resource_pool ADD COLUMN IF NOT EXISTS updated_at      TIMESTAMPTZ NOT NULL DEFAULT now();
