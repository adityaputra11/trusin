ALTER TABLE forward_rules
    ADD COLUMN IF NOT EXISTS rule_kind VARCHAR(20) NOT NULL DEFAULT 'provider',
    ADD COLUMN IF NOT EXISTS provider_id UUID REFERENCES forward_rules(id) ON DELETE CASCADE;

ALTER TABLE forward_rules
    ADD CONSTRAINT forward_rules_rule_kind_check CHECK (rule_kind IN ('provider', 'hook'));

CREATE INDEX IF NOT EXISTS idx_forward_rules_provider_hooks
    ON forward_rules (organization_id, provider_id)
    WHERE rule_kind = 'hook';
