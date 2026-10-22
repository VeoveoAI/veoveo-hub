INSERT INTO hub.selection (principal, server, branch, updated_at)
VALUES ($1, $2, $3, $4)
ON CONFLICT (principal, server) DO UPDATE
SET branch = EXCLUDED.branch, updated_at = EXCLUDED.updated_at
WHERE hub.selection.branch <> EXCLUDED.branch
