SELECT source_task_id, deployment
FROM hub.task_route
WHERE id = $1
