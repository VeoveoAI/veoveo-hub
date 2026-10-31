-- A push names the runner's tracked branch's name and latest pusher, whose token its client
-- issues for the linked repository, before the Hub restarts or creates its sandbox.
UPDATE hub.instance
SET tracked_branch_name = $2, linked_pusher = $3
WHERE deployment = $1 AND link IS NOT NULL
