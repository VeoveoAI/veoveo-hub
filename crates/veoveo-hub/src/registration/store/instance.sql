SELECT deployment, database
FROM hub.instance
WHERE token_digest = $1
