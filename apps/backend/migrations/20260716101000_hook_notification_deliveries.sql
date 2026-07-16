CREATE TABLE hook_notification_deliveries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    event_id UUID NOT NULL REFERENCES webhook_events(id) ON DELETE CASCADE,
    hook_id UUID NOT NULL REFERENCES forward_rules(id) ON DELETE CASCADE,
    destination_type VARCHAR(20) NOT NULL,
    status VARCHAR(20) NOT NULL,
    http_status INT,
    error TEXT,
    attempts INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX hook_notification_deliveries_event_idx
    ON hook_notification_deliveries (organization_id, event_id, created_at);
