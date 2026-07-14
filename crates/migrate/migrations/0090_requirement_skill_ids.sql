-- 需求关联的项目技能:创建需求时选择的 skill,派发时组合成指令下发到 agent runtime。
ALTER TABLE ms_requirement ADD COLUMN IF NOT EXISTS skill_ids TEXT[] NOT NULL DEFAULT '{}';
