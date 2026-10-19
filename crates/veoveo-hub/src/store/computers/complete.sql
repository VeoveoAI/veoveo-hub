UPDATE hub.computer SET ready = true, family_id = $2, handoff_id = NULL WHERE id = $1
