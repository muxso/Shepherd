CREATE TABLE ms_devops_activity_member (
 source_id TEXT NOT NULL REFERENCES ms_devops_source(id) ON DELETE CASCADE,
 name TEXT NOT NULL CHECK (btrim(name) <> ''),
 group_name TEXT,
 roles TEXT[] NOT NULL CHECK (cardinality(roles) > 0 AND roles <@ ARRAY['code','product','test']::text[]),
 enabled BOOLEAN NOT NULL DEFAULT true,
 PRIMARY KEY(source_id,name)
);
