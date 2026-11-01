-- The runner of one link of a branch.
SELECT deployment, database, linked_repository, tracked_branch, tracked_branch_name
FROM hub.instance
WHERE repository = $1 AND branch = $2 AND link = $3
