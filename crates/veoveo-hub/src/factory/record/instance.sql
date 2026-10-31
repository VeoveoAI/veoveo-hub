-- The ordinary instance of a branch, which serves it when its revision holds no links.
SELECT deployment, database
FROM hub.instance
WHERE repository = $1 AND branch = $2 AND link IS NULL
LIMIT 1
