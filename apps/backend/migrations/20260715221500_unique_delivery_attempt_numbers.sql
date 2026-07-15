WITH ranked_attempts AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY created_at ASC, id ASC) AS attempt_number
    FROM delivery_attempts
)
UPDATE delivery_attempts attempts
SET attempt_number = ranked_attempts.attempt_number
FROM ranked_attempts
WHERE attempts.id = ranked_attempts.id;

ALTER TABLE delivery_attempts
    ADD CONSTRAINT delivery_attempts_event_attempt_number_key
    UNIQUE (event_id, attempt_number);
