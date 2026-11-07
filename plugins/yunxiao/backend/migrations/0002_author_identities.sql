CREATE TABLE ms_devops_author_identity (
 source_id TEXT NOT NULL REFERENCES ms_devops_source(id) ON DELETE CASCADE,
 name TEXT NOT NULL CHECK (btrim(name) <> ''),
 group_name TEXT,
 PRIMARY KEY(source_id, name)
);
CREATE TABLE ms_devops_author_alias (
 source_id TEXT NOT NULL,
 alias TEXT NOT NULL CHECK (btrim(alias) <> ''),
 name TEXT NOT NULL,
 PRIMARY KEY(source_id, alias),
 FOREIGN KEY(source_id, name) REFERENCES ms_devops_author_identity(source_id, name) ON DELETE CASCADE
);
CREATE INDEX ix_devops_commit_author ON ms_devops_entity(source_id, (payload->>'author')) WHERE active AND kind='commit';

-- Resolve identities before filtering and aggregation without rewriting collected facts.
CREATE VIEW ms_devops_resolved_entity AS
SELECT e.source_id,e.kind,e.external_id,
 CASE WHEN i.name IS NULL THEN e.payload ELSE e.payload || jsonb_build_object(
   'author',i.name,
   'authorRaw',COALESCE(NULLIF(e.payload->>'authorRaw',''),e.payload->>'author'),
   'group',COALESCE(i.group_name,canonical_group.name,'待归属')) END AS payload,
 e.active,e.first_seen_at,e.observed_at
FROM ms_devops_entity e
LEFT JOIN LATERAL (
 SELECT a.name FROM ms_devops_author_alias a
 WHERE e.kind='commit' AND a.source_id=e.source_id
 AND a.alias IN (btrim(e.payload->>'authorRaw'),btrim(e.payload->>'author'))
 ORDER BY (a.alias=btrim(COALESCE(e.payload->>'authorRaw',''))) DESC LIMIT 1
) a ON true
LEFT JOIN ms_devops_author_identity i ON i.source_id=e.source_id AND i.name=a.name
LEFT JOIN LATERAL (
 SELECT CASE WHEN count(DISTINCT f.payload->>'group')=1 THEN min(f.payload->>'group') END AS name
 FROM ms_devops_entity f WHERE i.name IS NOT NULL AND i.group_name IS NULL
 AND f.source_id=e.source_id AND f.kind='commit' AND f.active AND f.payload->>'author'=i.name
 AND NULLIF(btrim(f.payload->>'group'),'') IS NOT NULL AND f.payload->>'group'<>'待归属'
) canonical_group ON true;
