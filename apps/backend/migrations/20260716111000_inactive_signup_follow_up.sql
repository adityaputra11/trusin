ALTER TABLE email_deliveries
    DROP CONSTRAINT IF EXISTS email_deliveries_status_check;

ALTER TABLE email_deliveries
    ADD CONSTRAINT email_deliveries_status_check
    CHECK (status IN ('pending', 'sending', 'sent', 'failed', 'skipped'));

CREATE INDEX IF NOT EXISTS idx_email_deliveries_inactive_follow_up
    ON email_deliveries (next_attempt_at)
    WHERE kind = 'inactive_follow_up' AND status IN ('pending', 'failed');
