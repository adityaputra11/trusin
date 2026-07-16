CREATE TABLE organization_weekly_digests (
    organization_id UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT false,
    last_sent_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
