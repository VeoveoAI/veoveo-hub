DELETE FROM hub.device_authorization WHERE id IN (SELECT id FROM hub.device_authorization WHERE expires_at <= $1 ORDER BY expires_at LIMIT 1000)
