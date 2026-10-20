-- Identity lookup only; the subsequent deployment-bound Core owner query authorizes cancellation.
SELECT deployment FROM tasks.task WHERE server = $1 AND id = $2 AND tenant = $3
