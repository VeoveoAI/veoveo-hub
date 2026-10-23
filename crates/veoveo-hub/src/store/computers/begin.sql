UPDATE hub.computer SET ready = false, family_id = NULL, handoff_id = $4, configure_request = $5, rotate_request = $6, lore_client_digest = $7
WHERE id = $1 AND tenant = $2 AND principal_id = $3 RETURNING id
