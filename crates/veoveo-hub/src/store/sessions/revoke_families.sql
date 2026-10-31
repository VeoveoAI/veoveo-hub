-- Revoke one bounded batch of the person's unexpired families that are not revoked yet.
-- The caller repeats until a batch comes back short.
UPDATE hub.refresh_family
SET revoked_at = $3, revocation_reason = $4
WHERE id IN (
    SELECT id FROM hub.refresh_family
    WHERE tenant = $1 AND principal_id = $2 AND revoked_at IS NULL AND expires_at > $3
    ORDER BY id
    LIMIT 1000
    FOR UPDATE
)
RETURNING id
