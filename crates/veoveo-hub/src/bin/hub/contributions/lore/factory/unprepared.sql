INSERT INTO hub.lore_prepare_failure
    (deployment, repository, branch, revision, pusher, message, failed_at)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (deployment, repository, branch)
DO UPDATE SET revision = excluded.revision, pusher = excluded.pusher,
    message = excluded.message, failed_at = excluded.failed_at
