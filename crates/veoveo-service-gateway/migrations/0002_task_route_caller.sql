-- A Task route of a server that calls other servers keeps the caller the Task acts for:
-- the client, actor, invocation authority and request context the Gateway
-- authenticated for the call that created the Task. A token exchange that names the
-- Task as its subject re-evaluates the installation's roles for this caller.
ALTER TABLE gateway.task_route ADD COLUMN delegated_caller jsonb;

COMMENT ON COLUMN gateway.task_route.delegated_caller IS
    'The caller a Task of a server with declared uses acts for while it runs; NULL for '
    'every other route. It goes with its route.';
