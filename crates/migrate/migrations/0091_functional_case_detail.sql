-- Functional case detail view: per-project display number, tags, audit
-- timestamps, field-level change history, and pre/post case dependencies.

ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS num bigint;
ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS tags jsonb NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS created_at timestamptz NOT NULL DEFAULT now();
ALTER TABLE ms_functional_case ADD COLUMN IF NOT EXISTS updated_at timestamptz NOT NULL DEFAULT now();

-- Backfill num per project; new numbers continue from MAX(num)+1 (first = 100001).
WITH numbered AS (
    SELECT id, 100000 + ROW_NUMBER() OVER (PARTITION BY project_id ORDER BY id) AS rn
    FROM ms_functional_case WHERE num IS NULL
)
UPDATE ms_functional_case c SET num = n.rn FROM numbered n WHERE c.id = n.id;

CREATE UNIQUE INDEX IF NOT EXISTS ux_functional_case_num
    ON ms_functional_case (project_id, num);

CREATE TABLE IF NOT EXISTS ms_functional_case_change (
    id bigserial PRIMARY KEY,
    case_id text NOT NULL,
    field text NOT NULL,
    old_value text NOT NULL DEFAULT '',
    new_value text NOT NULL DEFAULT '',
    actor text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS ix_functional_case_change_case
    ON ms_functional_case_change (case_id, created_at DESC);

-- Case dependencies: (case_id -> depends_on_id) means depends_on is a
-- precondition of case_id; the reverse direction lists post cases.
CREATE TABLE IF NOT EXISTS ms_case_dependency (
    project_id text NOT NULL,
    case_id text NOT NULL,
    depends_on_id text NOT NULL,
    created_by text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (case_id, depends_on_id)
);

CREATE INDEX IF NOT EXISTS ix_case_dependency_depends_on
    ON ms_case_dependency (depends_on_id);
