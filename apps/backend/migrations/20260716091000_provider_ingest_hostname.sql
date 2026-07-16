ALTER TABLE forward_rules
    ADD COLUMN IF NOT EXISTS ingest_hostname VARCHAR(253);

CREATE INDEX IF NOT EXISTS idx_forward_rules_ingest_hostname
    ON forward_rules (organization_id, ingest_hostname)
    WHERE ingest_hostname IS NOT NULL;
