-- A push names the instance's latest pusher, whose instances count toward the per-user
-- cap.
UPDATE hub.instance
SET pusher = $2
WHERE deployment = $1
RETURNING branch
