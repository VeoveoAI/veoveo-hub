-- The instance whose client presents a secret with this digest, its latest pusher and
-- its repository, and for a runner its linked repository and that repository's tracked
-- branch's latest pusher.
SELECT deployment, pusher, repository, linked_repository, linked_pusher
FROM hub.instance
WHERE client_digest = $1
