SELECT deployment, database
FROM hub.instance
WHERE repository = $1 AND branch = $2
