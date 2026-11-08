CREATE TABLE ms_rag_channel (
 id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
 name TEXT NOT NULL,
 service_user_id TEXT NOT NULL UNIQUE REFERENCES ms_user(id),
 enabled BOOLEAN NOT NULL DEFAULT true,
 allow_binding BOOLEAN NOT NULL DEFAULT false,
 updated_by TEXT NOT NULL,
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE ms_rag_channel_conversation (
 channel_id TEXT NOT NULL REFERENCES ms_rag_channel(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('group','direct')),
 external_id TEXT NOT NULL,
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 enabled BOOLEAN NOT NULL DEFAULT true,
 PRIMARY KEY(channel_id,kind,external_id)
);
CREATE TABLE ms_rag_channel_status (
 channel_id TEXT PRIMARY KEY REFERENCES ms_rag_channel(id) ON DELETE CASCADE,
 authenticated BOOLEAN NOT NULL DEFAULT false,
 heartbeat_age_seconds DOUBLE PRECISION,
 active_queries INTEGER NOT NULL DEFAULT 0,
 last_message_at BIGINT,
 reported_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE ms_rag_channel_receipt (
 channel_id TEXT NOT NULL REFERENCES ms_rag_channel(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('group','direct')),
 external_id TEXT NOT NULL,
 sender_id TEXT NOT NULL,
 seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(channel_id,kind,external_id)
);

CREATE TABLE ms_rag_channel_binding_code (
 code_hash TEXT PRIMARY KEY,
 channel_id TEXT NOT NULL REFERENCES ms_rag_channel(id) ON DELETE CASCADE,
 project_id TEXT NOT NULL REFERENCES ms_project(id),
 expires_at TIMESTAMPTZ NOT NULL,
 used_at TIMESTAMPTZ,
 created_by TEXT NOT NULL,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
