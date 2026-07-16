ALTER TABLE organization_destinations
    DROP CONSTRAINT IF EXISTS organization_destinations_kind_check;

UPDATE forward_rules
SET active = FALSE
WHERE rule_kind = 'hook' AND destination_type = 'email';

DELETE FROM organization_destinations
WHERE kind = 'email';

UPDATE organization_weekly_digests
SET enabled = FALSE,
    last_sent_at = NULL,
    updated_at = NOW()
WHERE enabled = TRUE;

ALTER TABLE organization_destinations
    ADD CONSTRAINT organization_destinations_kind_check
    CHECK (kind IN ('slack', 'telegram'));
