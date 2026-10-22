SELECT revision, pusher, message
FROM hub.lore_prepare_failure
WHERE deployment = $1 AND repository = $2 AND branch = $3
