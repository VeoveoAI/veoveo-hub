-- A push names the instance's latest pusher, whose instances count toward the per-user
-- cap, and the restart or creation that follows ends what the server served.
UPDATE hub.instance
SET pusher = $2, served_at = NULL
WHERE deployment = $1
RETURNING branch
