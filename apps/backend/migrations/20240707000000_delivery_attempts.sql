-- Per-attempt delivery log. One row per outbound HTTP attempt, so the UI can
-- render a timeline of retries incl. http status, duration, and errors.
CREATE TABLE IF NOT EXISTS delivery_attempts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_id UUID NOT NULL REFERENCES webhook_events(id) ON DELETE CASCADE,
    attempt_number INT NOT NULL,          -- 1-based; ~ webhook_events.retry_count before this attempt
    status VARCHAR(50) NOT NULL,          -- delivered | failed | retrying | network_error
    http_status INT,                      -- remote response code, NULL on network error
    response_headers JSONB,
    response_body TEXT,
    error TEXT,                           -- transport error message when applicable
    duration_ms INT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_delivery_attempts_event
    ON delivery_attempts(event_id, attempt_number);
