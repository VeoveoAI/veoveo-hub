-- The runner of one link of a branch.
SELECT deployment, database
FROM hub.instance
WHERE repository = $1 AND branch = $2 AND link = $3
