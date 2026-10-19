-- Lock the current Computer first, matching custody's lock order. A stale cookie
-- cannot revoke a family after the next explicit handoff has begun/completed.
WITH owner AS MATERIALIZED (
    SELECT family_id FROM hub.computer
    WHERE id = $1 AND tenant = $2 AND principal_id = $3
      AND ready AND handoff_id IS NULL AND family_id = $4
    FOR UPDATE
)
UPDATE hub.refresh_family AS family
SET revoked_at = coalesce(family.revoked_at, $5),
    revocation_reason = coalesce(family.revocation_reason, $6)
FROM owner
WHERE family.id = owner.family_id
RETURNING family.id
