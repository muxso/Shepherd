-- RAG per-document visibility via named "visibility groups". Retrieval was project-scoped only; a
-- document now belongs to zero or more groups, and a group bundles RBAC role names. A caller sees a
-- document when they uploaded it, are an admin, or hold a role in one of the document's groups.
--
-- Live reference (not a snapshot): documents store group ids, so editing a group's roles re-scopes
-- every document in it. Empty visibility_groups = restricted to the uploader + admins (not everyone),
-- so legacy rows default to hidden until assigned a group.

-- A visibility group = a reusable, org-wide bundle of role names (e.g. 对外 = {销售, 产品}).
CREATE TABLE IF NOT EXISTS rag_visibility_group (
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name        TEXT NOT NULL,
    role_names  TEXT[] NOT NULL DEFAULT '{}',
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);
-- Overlap (&&) lookups of "which groups include one of the caller's roles" use a GIN index.
CREATE INDEX IF NOT EXISTS idx_rag_group_roles ON rag_visibility_group USING GIN (role_names);

ALTER TABLE ms_rag_document ADD COLUMN IF NOT EXISTS owner_id TEXT;
ALTER TABLE ms_rag_document ADD COLUMN IF NOT EXISTS visibility_groups TEXT[] NOT NULL DEFAULT '{}';
CREATE INDEX IF NOT EXISTS idx_rag_document_groups ON ms_rag_document USING GIN (visibility_groups);
