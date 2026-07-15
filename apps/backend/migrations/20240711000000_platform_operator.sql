ALTER TABLE users
    ADD COLUMN IF NOT EXISTS is_platform_operator BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS subscriber_name VARCHAR(120) NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS billing_contact_name VARCHAR(120) NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS billing_contact_email VARCHAR(255) NOT NULL DEFAULT '';

UPDATE organizations
SET subscriber_name = name
WHERE subscriber_name = '';

UPDATE organizations o
SET billing_contact_name = COALESCE((
        SELECT COALESCE(u.display_name, u.username, '')
        FROM users u
        WHERE u.organization_id = o.id AND u.role = 'admin'
        ORDER BY u.created_at ASC
        LIMIT 1
    ), ''),
    billing_contact_email = COALESCE((
        SELECT u.email
        FROM users u
        WHERE u.organization_id = o.id AND u.role = 'admin'
        ORDER BY u.created_at ASC
        LIMIT 1
    ), '')
WHERE o.billing_contact_name = '' AND o.billing_contact_email = '';

CREATE INDEX IF NOT EXISTS idx_users_platform_operator
    ON users (is_platform_operator) WHERE is_platform_operator = TRUE;
CREATE INDEX IF NOT EXISTS idx_organizations_subscription_status
    ON organizations (subscription_status, created_at DESC);
