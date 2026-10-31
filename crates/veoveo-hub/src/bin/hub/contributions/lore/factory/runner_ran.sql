-- The link's runner was applied, or the link left the branch, which clears its failure.
DELETE FROM hub.lore_runner_failure
WHERE deployment = $1 AND repository = $2 AND branch = $3 AND link = $4
