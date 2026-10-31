SELECT linked_repository, revision, message
FROM hub.lore_runner_failure
WHERE deployment = $1 AND repository = $2 AND branch = $3 AND link = $4
