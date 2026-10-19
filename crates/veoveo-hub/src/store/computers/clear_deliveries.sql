-- Clear one bounded batch of this revoked family's successor deliveries. The
-- caller retains the family lock and repeats until empty before audit/commit.
UPDATE hub.refresh_token
SET delivery_envelope = NULL, delivery_expires_at = NULL
WHERE id IN (
    SELECT id FROM hub.refresh_token
    WHERE family = $1 AND delivery_envelope IS NOT NULL
    ORDER BY generation
    LIMIT 1000
    FOR UPDATE
)
