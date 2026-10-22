SELECT watcher, failed_message
FROM hub.lore_watch
WHERE deployment = $1 AND repository = $2
