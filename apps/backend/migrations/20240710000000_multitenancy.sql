CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(120) NOT NULL,
    slug VARCHAR(80) NOT NULL UNIQUE,
    plan_code VARCHAR(40) NOT NULL DEFAULT 'free',
    subscription_status VARCHAR(40) NOT NULL DEFAULT 'active',
    billing_period_start TIMESTAMPTZ NOT NULL DEFAULT date_trunc('month', NOW()),
    billing_period_end TIMESTAMPTZ NOT NULL DEFAULT (date_trunc('month', NOW()) + INTERVAL '1 month'),
    default_target_url TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO organizations (id, name, slug)
SELECT uuid_generate_v4(), 'Terusin Default', 'default'
WHERE NOT EXISTS (SELECT 1 FROM organizations WHERE slug = 'default');

ALTER TABLE users ADD COLUMN IF NOT EXISTS organization_id UUID;
ALTER TABLE forward_rules ADD COLUMN IF NOT EXISTS organization_id UUID;
ALTER TABLE webhook_events ADD COLUMN IF NOT EXISTS organization_id UUID;
ALTER TABLE delivery_attempts ADD COLUMN IF NOT EXISTS organization_id UUID;
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS organization_id UUID;
ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS organization_id UUID;
ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS scopes TEXT[] NOT NULL DEFAULT ARRAY['events:read', 'webhooks:send', 'rules:read', 'rules:write'];

UPDATE users SET organization_id = (SELECT id FROM organizations WHERE slug = 'default') WHERE organization_id IS NULL;
UPDATE forward_rules SET organization_id = (SELECT id FROM organizations WHERE slug = 'default') WHERE organization_id IS NULL;
UPDATE webhook_events SET organization_id = (SELECT id FROM organizations WHERE slug = 'default') WHERE organization_id IS NULL;
UPDATE delivery_attempts d SET organization_id = e.organization_id FROM webhook_events e WHERE d.event_id = e.id AND d.organization_id IS NULL;
UPDATE audit_logs SET organization_id = (SELECT id FROM organizations WHERE slug = 'default') WHERE organization_id IS NULL;
UPDATE api_tokens t SET organization_id = u.organization_id FROM users u WHERE t.user_id = u.id AND t.organization_id IS NULL;

ALTER TABLE users ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE forward_rules ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE webhook_events ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE delivery_attempts ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE audit_logs ALTER COLUMN organization_id SET NOT NULL;
ALTER TABLE api_tokens ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE users ADD CONSTRAINT users_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE RESTRICT;
ALTER TABLE forward_rules ADD CONSTRAINT forward_rules_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
ALTER TABLE webhook_events ADD CONSTRAINT webhook_events_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
ALTER TABLE delivery_attempts ADD CONSTRAINT delivery_attempts_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
ALTER TABLE audit_logs ADD CONSTRAINT audit_logs_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;
ALTER TABLE api_tokens ADD CONSTRAINT api_tokens_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;

CREATE INDEX idx_users_organization ON users (organization_id);
CREATE INDEX idx_forward_rules_organization ON forward_rules (organization_id, created_at);
CREATE INDEX idx_webhook_events_organization_created ON webhook_events (organization_id, created_at DESC);
CREATE INDEX idx_webhook_events_organization_status ON webhook_events (organization_id, status);
CREATE INDEX idx_delivery_attempts_organization ON delivery_attempts (organization_id, event_id);
CREATE INDEX idx_audit_logs_organization_created ON audit_logs (organization_id, created_at DESC);
CREATE INDEX idx_api_tokens_organization ON api_tokens (organization_id) WHERE revoked_at IS NULL;

CREATE TABLE organization_domains (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    hostname VARCHAR(253) NOT NULL UNIQUE,
    verification_token VARCHAR(96) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT organization_domains_status_check CHECK (status IN ('pending', 'verified', 'active', 'failed')),
    CONSTRAINT organization_domains_hostname_lowercase CHECK (hostname = lower(hostname))
);

CREATE INDEX idx_organization_domains_organization ON organization_domains (organization_id, created_at DESC);

CREATE TABLE organization_usage (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    period_start DATE NOT NULL,
    events_accepted INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (organization_id, period_start)
);
