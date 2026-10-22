-- The instance whose client presents a secret with this digest, its latest pusher and
-- its repository.
SELECT deployment, pusher, repository
FROM hub.instance
WHERE client_digest = $1
