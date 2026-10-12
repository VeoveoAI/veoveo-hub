UPDATE gateway.task_route
SET created_at = $2::timestamptz - interval '1 hour', expires_at = $2
WHERE id = $1
