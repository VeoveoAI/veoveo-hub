SELECT deployment
FROM hub.instance
WHERE token_digest = $1
