CREATE TABLE user_oauth_identities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(50) NOT NULL,
    subject VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (provider, subject)
);

INSERT INTO user_oauth_identities (user_id, provider, subject, email)
SELECT id, oauth_provider, oauth_subject, COALESCE(email, '')
FROM users
WHERE oauth_provider IS NOT NULL AND oauth_subject IS NOT NULL
ON CONFLICT (provider, subject) DO NOTHING;

CREATE INDEX idx_user_oauth_identities_user ON user_oauth_identities (user_id);

CREATE TABLE email_deliveries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(50) NOT NULL,
    recipient VARCHAR(255) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    provider_message_id VARCHAR(255),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error TEXT,
    sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT email_deliveries_status_check CHECK (status IN ('pending', 'sending', 'sent', 'failed')),
    UNIQUE (user_id, kind)
);

CREATE INDEX idx_email_deliveries_pending ON email_deliveries (status, next_attempt_at)
    WHERE status IN ('pending', 'failed');
