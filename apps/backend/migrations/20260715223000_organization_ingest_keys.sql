ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS ingest_key VARCHAR(64);

UPDATE organizations
SET ingest_key = replace(uuid_generate_v4()::text, '-', '')
WHERE ingest_key IS NULL OR ingest_key = '';

ALTER TABLE organizations
    ALTER COLUMN ingest_key SET NOT NULL,
    ALTER COLUMN ingest_key SET DEFAULT replace(uuid_generate_v4()::text, '-', '');

ALTER TABLE organizations
    ADD CONSTRAINT organizations_ingest_key_key UNIQUE (ingest_key);
