SELECT count(*) AS "remaining!" FROM hub.refresh_token
WHERE family = $1 AND delivery_envelope IS NOT NULL
