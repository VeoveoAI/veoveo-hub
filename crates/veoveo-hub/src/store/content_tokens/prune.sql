DELETE FROM hub.content_token WHERE digest IN (
    SELECT digest FROM hub.content_token WHERE expires_at <= $1 ORDER BY expires_at LIMIT 100
)
