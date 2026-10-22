UPDATE hub.instance
SET token_digest = $2
WHERE deployment = $1
