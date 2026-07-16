CREATE TABLE organization_destinations (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    kind VARCHAR(20) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, kind),
    CONSTRAINT organization_destinations_kind_check CHECK (kind IN ('slack', 'telegram', 'email'))
);

INSERT INTO organization_destinations (organization_id, kind, config)
SELECT organization_id, destination_type, destination_config
FROM forward_rules
WHERE rule_kind = 'hook'
  AND destination_type IN ('slack', 'telegram', 'email')
  AND destination_config <> '{}'::jsonb
ON CONFLICT (organization_id, kind) DO NOTHING;

UPDATE forward_rules
SET destination_config = '{}'::jsonb
WHERE rule_kind = 'hook' AND destination_type IN ('slack', 'telegram', 'email');
