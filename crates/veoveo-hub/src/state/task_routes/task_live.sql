-- Whether one server's Task in core's Task runtime still runs in the caller's tenant. A
-- Task waiting for its cancellation to settle no longer acts for its caller.
SELECT EXISTS (
    SELECT 1 FROM tasks.task
    WHERE server = $1 AND id = $2 AND tenant = $3 AND status IN ('queued', 'running', 'waiting')
) AS "live!"
