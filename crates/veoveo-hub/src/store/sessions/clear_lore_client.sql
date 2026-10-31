-- Lock the person's Computer first, matching custody's lock order, and clear its Lore
-- client, so no secret mints a Lore token for it until the next handoff gives a new one.
UPDATE hub.computer SET lore_client_digest = NULL
WHERE tenant = $1 AND principal_id = $2
RETURNING id
