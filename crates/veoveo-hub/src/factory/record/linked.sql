-- The runner's complete linked target and latest pusher. Retargeting records this
-- only after the previous sandbox is absent, before creating new credentials.
UPDATE hub.instance
SET linked_repository = $2, tracked_branch = $3, tracked_branch_name = $4, linked_pusher = $5
WHERE deployment = $1 AND link IS NOT NULL
