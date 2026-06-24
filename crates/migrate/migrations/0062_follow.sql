-- 关注人(通用):任意业务对象都能被 (project_id, entity_type, entity_id) 定位,
-- 用户对其关注 / 取消关注。四元组即主键,天然幂等、零重复。
CREATE TABLE IF NOT EXISTS ms_follow (
    project_id  text NOT NULL,
    entity_type text NOT NULL,                       -- bug / requirement / case / ...
    entity_id   text NOT NULL,
    user_id     text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, entity_type, entity_id, user_id)
);

-- 「某对象的关注人」:按对象定位。
CREATE INDEX IF NOT EXISTS idx_follow_entity
    ON ms_follow (project_id, entity_type, entity_id);

-- 「我关注的对象」:按 (项目, 用户) 定位,可再按类型过滤。
CREATE INDEX IF NOT EXISTS idx_follow_user
    ON ms_follow (project_id, user_id, entity_type);
