-- 接口定义增人类可读编号 num(列表/详情的【101093】式 ID),由序列分配。
CREATE SEQUENCE IF NOT EXISTS ms_api_definition_num_seq START 100001;
ALTER TABLE ms_api_definition ADD COLUMN IF NOT EXISTS num BIGINT;

-- 回填:按创建时间(无则按 id)稳定顺序给既有行编号。
WITH ordered AS (
    SELECT id, row_number() OVER (ORDER BY created_at, id) AS rn
    FROM ms_api_definition
    WHERE num IS NULL
)
UPDATE ms_api_definition d
SET num = 100000 + o.rn
FROM ordered o
WHERE d.id = o.id;

-- 推进序列到当前最大编号之后,避免新插入与回填冲突。
SELECT setval(
    'ms_api_definition_num_seq',
    GREATEST((SELECT COALESCE(MAX(num), 100000) FROM ms_api_definition), 100000)
);

-- 新插入自动取序列值,并锁定非空。
ALTER TABLE ms_api_definition ALTER COLUMN num SET DEFAULT nextval('ms_api_definition_num_seq');
ALTER TABLE ms_api_definition ALTER COLUMN num SET NOT NULL;
