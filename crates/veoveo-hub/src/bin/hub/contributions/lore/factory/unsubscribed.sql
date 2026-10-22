-- A subscription as the watcher failed.
UPDATE hub.lore_watch
SET failed_message = $4, updated_at = $5
WHERE deployment = $1 AND repository = $2 AND watcher = $3
