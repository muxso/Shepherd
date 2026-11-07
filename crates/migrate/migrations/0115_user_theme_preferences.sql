-- Session principals can include environment-managed administrators without a user row.
CREATE TABLE ms_user_theme (
    user_id TEXT PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode IN ('light','dark')),
    primary_color TEXT NOT NULL CHECK (primary_color ~ '^#[0-9A-Fa-f]{6}$'),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
