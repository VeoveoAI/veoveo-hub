-- A new instance of a branch, or the runner of one of its links with the linked repository
-- and the branch of it the link tracks.
INSERT INTO hub.instance (
    deployment, token_digest, repository, database, branch, created_at, link, linked_repository,
    tracked_branch, tracked_branch_name
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
