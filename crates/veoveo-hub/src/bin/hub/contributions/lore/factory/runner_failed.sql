-- The Hub could not run a link of the branch at this revision.
INSERT INTO hub.lore_runner_failure
    (deployment, repository, branch, link, linked_repository, revision, message, failed_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (deployment, repository, branch, link)
DO UPDATE SET linked_repository = excluded.linked_repository, revision = excluded.revision,
    message = excluded.message, failed_at = excluded.failed_at
