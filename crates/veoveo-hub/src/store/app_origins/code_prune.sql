DELETE FROM hub.app_origin_code WHERE code_digest IN (
    SELECT code_digest FROM hub.app_origin_code WHERE expires_at <= $1 ORDER BY expires_at LIMIT 1000
)
