ALTER TABLE forward_rules
    ADD COLUMN IF NOT EXISTS destination_type VARCHAR(20) NOT NULL DEFAULT 'webhook',
    ADD COLUMN IF NOT EXISTS destination_config JSONB NOT NULL DEFAULT '{}';

ALTER TABLE forward_rules
    ADD CONSTRAINT forward_rules_destination_type_check
    CHECK (destination_type IN ('webhook', 'slack', 'telegram', 'email'));
