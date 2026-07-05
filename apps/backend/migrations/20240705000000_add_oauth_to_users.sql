-- Add OAuth columns to support Google sign-in alongside the existing
-- username/password auth. password_hash becomes nullable because users who
-- sign in exclusively via Google will never have a password.
-- A user is matched by (oauth_provider, oauth_subject) on Google callback.

ALTER TABLE users
    ALTER COLUMN password_hash DROP NOT NULL;

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS email VARCHAR(255) UNIQUE,
    ADD COLUMN IF NOT EXISTS display_name VARCHAR(255),
    ADD COLUMN IF NOT EXISTS avatar_url TEXT,
    ADD COLUMN IF NOT EXISTS oauth_provider VARCHAR(50),
    ADD COLUMN IF NOT EXISTS oauth_subject VARCHAR(255);

-- Allow username to be nullable too: Google-only users may not have a
-- username until they set one. We still keep the UNIQUE constraint.
ALTER TABLE users ALTER COLUMN username DROP NOT NULL;

-- Helpful for looking up OAuth users in one shot.
CREATE INDEX IF NOT EXISTS idx_users_oauth ON users (oauth_provider, oauth_subject)
    WHERE oauth_provider IS NOT NULL;
