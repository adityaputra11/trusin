-- Optional per-rule secret used to HMAC-sign outbound deliveries.
-- NULL (default) means no signature header is added for this rule.
ALTER TABLE forward_rules
    ADD COLUMN IF NOT EXISTS signing_secret TEXT;
