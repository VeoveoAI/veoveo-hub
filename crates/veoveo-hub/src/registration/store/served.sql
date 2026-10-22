-- No row changes for a deployment outside an instance.
UPDATE hub.instance
SET served_at = now()
WHERE deployment = $1
