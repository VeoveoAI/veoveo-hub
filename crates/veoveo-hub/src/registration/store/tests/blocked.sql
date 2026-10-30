SELECT waiter.pid AS "pid!", waiter.query AS "query!", waiter.wait_event AS "event!"
FROM pg_stat_activity AS waiter
WHERE waiter.datname = current_database()
  AND waiter.wait_event_type = 'Lock'
  AND $1::integer = ANY (pg_blocking_pids(waiter.pid))
ORDER BY waiter.pid
