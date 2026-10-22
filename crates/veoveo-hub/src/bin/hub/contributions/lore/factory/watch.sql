-- The caller turned the repository on: the Hub watches it as them from now on.
INSERT INTO hub.lore_watch (deployment, repository, watcher, failed_message, updated_at)
VALUES ($1, $2, $3, NULL, $4)
ON CONFLICT (deployment, repository)
DO UPDATE SET watcher = excluded.watcher, failed_message = NULL, updated_at = excluded.updated_at
