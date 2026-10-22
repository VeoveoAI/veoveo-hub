UPDATE hub.instance
SET served_at = now()
WHERE deployment = $1
