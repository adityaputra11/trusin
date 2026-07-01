CREATE TABLE IF NOT EXISTS forward_rules (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    source_pattern VARCHAR(255) NOT NULL DEFAULT '*',
    target_url TEXT NOT NULL,
    method VARCHAR(10) NOT NULL DEFAULT 'POST',
    headers JSONB NOT NULL DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

INSERT INTO forward_rules (name, source_pattern, target_url, active)
VALUES ('Default', '*', '', true);
