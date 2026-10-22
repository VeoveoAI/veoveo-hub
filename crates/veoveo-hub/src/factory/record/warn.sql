UPDATE hub.instance
SET warning = $2
WHERE deployment = $1
