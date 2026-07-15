ALTER TABLE forward_rules
    ADD COLUMN IF NOT EXISTS trigger_on VARCHAR(20) NOT NULL DEFAULT 'success';

ALTER TABLE forward_rules
    ADD CONSTRAINT forward_rules_trigger_on_check
    CHECK (trigger_on IN ('success', 'failure'));
