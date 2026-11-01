-- The Hub restarts or creates the instance's sandbox, which ends what its server served.
UPDATE hub.instance
SET served_at = NULL
WHERE deployment = $1
