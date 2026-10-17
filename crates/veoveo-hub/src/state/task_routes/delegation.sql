-- veoveo: all tenants (a server names its own Task by the identifier it issued; the route's tenant is the caller's, which the stored caller carries)
-- The caller one server's Task acts for, while its route lives. A server issues each
-- Task identifier once, so a second row means the identifier is ambiguous.
SELECT r.profile, r.delegated_caller AS "delegated_caller!", r.expires_at
FROM hub.task_route AS r
WHERE r.server = $1 AND r.source_task_id = $2 AND r.expires_at > $3
    AND r.delegated_caller IS NOT NULL
LIMIT 2
