-- A subscription as the watcher succeeded, which clears its last failure.
UPDATE hub.lore_watch
SET failed_message = NULL, updated_at = $4
WHERE deployment = $1 AND repository = $2 AND watcher = $3
