-- Build a compact read model once when collected facts change, not on every request.
CREATE TABLE ms_devops_statistics_entity (LIKE ms_devops_entity INCLUDING ALL);
ALTER TABLE ms_devops_statistics_entity ADD FOREIGN KEY(source_id) REFERENCES ms_devops_source(id) ON DELETE CASCADE;
CREATE TABLE ms_devops_statistics_state (
 source_id TEXT PRIMARY KEY REFERENCES ms_devops_source(id) ON DELETE CASCADE,
 revision BIGINT NOT NULL DEFAULT 1
);
CREATE FUNCTION ms_devops_statistics_payload(p JSONB) RETURNS JSONB LANGUAGE SQL IMMUTABLE AS $$
 SELECT COALESCE((SELECT jsonb_object_agg(key,value) FROM jsonb_each(p) WHERE key=ANY(ARRAY[
 'title','serial','owner','author','authorRaw','group','status','priority','due',
 'createdAt','updatedAt','statusChangedAt','authoredAt','requirementIds','parentId','parentSerial',
 'repoId','repo','sha','additions','deletions','isMerge','parentIds','branches',
 'repositoryLanguages','repositoryLanguageBytes','repositoryLanguageSource','repositoryLanguageCommit',
 'repositoryUrl','repoUrl','reopenHistory','creatorInfo'])), '{}'::jsonb)
 || jsonb_build_object(
 'creator',COALESCE(NULLIF(p->>'creator',''),p#>>'{raw,creator,name}',p#>>'{raw,creator,displayName}'),
 'url',COALESCE(NULLIF(p->>'url',''),NULLIF(p#>>'{raw,url}',''),NULLIF(p#>>'{raw,webUrl}',''),NULLIF(p#>>'{raw,web_url}','')),
 'customFieldValues',COALESCE((SELECT jsonb_agg(f) FROM jsonb_array_elements(
 CASE WHEN jsonb_typeof(p->'customFieldValues')='array' THEN p->'customFieldValues'
 WHEN jsonb_typeof(p#>'{raw,customFieldValues}')='array' THEN p#>'{raw,customFieldValues}' ELSE '[]'::jsonb END) f
 WHERE f->>'fieldName' IN ('开发者','测试人员','协同人','协作人','需求提出人','缺陷责任人','缺陷类型')),'[]'::jsonb))
$$;
CREATE FUNCTION ms_devops_project_statistics() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN
  DELETE FROM ms_devops_statistics_entity WHERE source_id=OLD.source_id AND kind=OLD.kind AND external_id=OLD.external_id;
 ELSE
  INSERT INTO ms_devops_statistics_entity(source_id,kind,external_id,payload,active,first_seen_at,observed_at)
  VALUES(NEW.source_id,NEW.kind,NEW.external_id,ms_devops_statistics_payload(NEW.payload),NEW.active,NEW.first_seen_at,NEW.observed_at)
  ON CONFLICT(source_id,kind,external_id) DO UPDATE SET payload=EXCLUDED.payload,active=EXCLUDED.active,observed_at=EXCLUDED.observed_at;
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER project_statistics AFTER INSERT OR UPDATE OR DELETE ON ms_devops_entity
 FOR EACH ROW EXECUTE FUNCTION ms_devops_project_statistics();
INSERT INTO ms_devops_statistics_entity SELECT source_id,kind,external_id,ms_devops_statistics_payload(payload),active,first_seen_at,observed_at FROM ms_devops_entity;
INSERT INTO ms_devops_statistics_state(source_id) SELECT id FROM ms_devops_source;
CREATE FUNCTION ms_devops_invalidate_statistics() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN
  INSERT INTO ms_devops_statistics_state(source_id) SELECT DISTINCT r.source_id FROM old_rows r JOIN ms_devops_source s ON s.id=r.source_id
  ON CONFLICT(source_id) DO UPDATE SET revision=ms_devops_statistics_state.revision+1;
 ELSE
  INSERT INTO ms_devops_statistics_state(source_id) SELECT DISTINCT r.source_id FROM new_rows r JOIN ms_devops_source s ON s.id=r.source_id
  ON CONFLICT(source_id) DO UPDATE SET revision=ms_devops_statistics_state.revision+1;
 END IF;
 RETURN NULL;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['ms_devops_entity','ms_devops_sync_run','ms_devops_author_identity','ms_devops_author_alias','ms_devops_activity_member','ms_devops_activity_review_policy'] LOOP
  EXECUTE format('CREATE TRIGGER statistics_insert AFTER INSERT ON %I REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ms_devops_invalidate_statistics()',t);
  EXECUTE format('CREATE TRIGGER statistics_update AFTER UPDATE ON %I REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ms_devops_invalidate_statistics()',t);
  EXECUTE format('CREATE TRIGGER statistics_delete AFTER DELETE ON %I REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ms_devops_invalidate_statistics()',t);
 END LOOP;
END $$;

-- Resolve aliases with indexed joins and aggregate fallback groups once.
CREATE VIEW ms_devops_statistics_resolved AS
SELECT e.source_id,e.kind,e.external_id,
 CASE WHEN i.name IS NULL THEN e.payload ELSE e.payload || jsonb_build_object(
   'author',i.name,
   'authorRaw',COALESCE(NULLIF(e.payload->>'authorRaw',''),e.payload->>'author'),
   'group',COALESCE(i.group_name,canonical_group.name,'待归属')) END AS payload,
 e.active,e.first_seen_at,e.observed_at
FROM ms_devops_statistics_entity e
LEFT JOIN ms_devops_author_alias raw_alias
 ON e.kind='commit' AND raw_alias.source_id=e.source_id
 AND raw_alias.alias=btrim(e.payload->>'authorRaw')
LEFT JOIN ms_devops_author_alias normalized_alias
 ON e.kind='commit' AND raw_alias.name IS NULL AND normalized_alias.source_id=e.source_id
 AND normalized_alias.alias=btrim(e.payload->>'author')
LEFT JOIN ms_devops_author_identity i
 ON i.source_id=e.source_id AND i.name=COALESCE(raw_alias.name,normalized_alias.name)
LEFT JOIN (
 SELECT source_id,payload->>'author' AS author,
   CASE WHEN count(DISTINCT payload->>'group')=1 THEN min(payload->>'group') END AS name
 FROM ms_devops_statistics_entity
 WHERE active AND kind='commit' AND NULLIF(btrim(payload->>'group'),'') IS NOT NULL
   AND payload->>'group'<>'待归属'
 GROUP BY source_id,payload->>'author'
) canonical_group
 ON i.group_name IS NULL AND canonical_group.source_id=e.source_id AND canonical_group.author=i.name;

CREATE FUNCTION ms_devops_source_statistics_revision() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO ms_devops_statistics_state(source_id) SELECT id FROM new_rows
 ON CONFLICT(source_id) DO UPDATE SET revision=ms_devops_statistics_state.revision+1;
 RETURN NULL;
END $$;
CREATE TRIGGER statistics_source_insert AFTER INSERT ON ms_devops_source REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ms_devops_source_statistics_revision();
CREATE TRIGGER statistics_source_update AFTER UPDATE ON ms_devops_source REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ms_devops_source_statistics_revision();

-- Bounded, precomputed common responses survive Redis restarts.
CREATE TABLE ms_devops_statistics_snapshot (
 cache_key TEXT PRIMARY KEY,
 source_id TEXT NOT NULL REFERENCES ms_devops_source(id) ON DELETE CASCADE,
 revision BIGINT NOT NULL,
 payload JSONB NOT NULL,
 expires_at TIMESTAMPTZ NOT NULL DEFAULT now()+interval '90 seconds'
);
CREATE INDEX ix_devops_statistics_snapshot_expiry ON ms_devops_statistics_snapshot(expires_at);
