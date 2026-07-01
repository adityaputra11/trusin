ALTER TABLE webhook_events ADD COLUMN IF NOT EXISTS response_status INTEGER;
ALTER TABLE webhook_events ADD COLUMN IF NOT EXISTS response_headers JSONB;
ALTER TABLE webhook_events ADD COLUMN IF NOT EXISTS response_body TEXT;
