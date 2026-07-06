-- API tokens for CLI / MCP pairing. Each token is a 256-bit random string
-- shown once to the user (the DB only stores its sha256 hash so lookups are
-- index-able and the secret is never persisted). Tokens are per-user,
-- named (e.g. "MacBook CLI"), and individually revocable.

CREATE TABLE api_tokens (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         VARCHAR(120) NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,            -- sha256(token) hex
    last_used_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at   TIMESTAMPTZ
);

CREATE INDEX idx_api_tokens_user ON api_tokens (user_id) WHERE revoked_at IS NULL;
