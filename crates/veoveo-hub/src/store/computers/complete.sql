UPDATE hub.computer SET ready = true, family_id = $2, handoff_id = NULL WHERE id = $1 AND tenant = $3 AND principal_id = $4
