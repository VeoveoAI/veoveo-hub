DELETE FROM hub.app_origin_code WHERE code_digest = $1 AND state_digest = $2 AND server = $3 AND expires_at > $4 RETURNING computer_id, family_id, return_path
