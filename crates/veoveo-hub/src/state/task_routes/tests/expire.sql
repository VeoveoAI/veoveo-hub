-- veoveo: all tenants (a unit test expires a route of its own fixture database by its identifier)
UPDATE hub.task_route
SET created_at = $2::timestamptz - interval '1 hour', expires_at = $2
WHERE id = $1
