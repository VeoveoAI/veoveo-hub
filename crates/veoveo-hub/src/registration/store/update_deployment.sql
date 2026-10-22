UPDATE hub.deployment
SET metadata = $2
WHERE id = $1
