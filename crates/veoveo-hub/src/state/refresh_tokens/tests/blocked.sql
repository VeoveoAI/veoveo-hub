SELECT EXISTS (
    SELECT 1 FROM pg_stat_activity AS waiter
    WHERE waiter.datname = current_database()
      AND waiter.wait_event_type = 'Lock'
      AND $1::integer = ANY (pg_blocking_pids(waiter.pid))
      AND waiter.query LIKE '%FROM%hub.refresh_family%FOR%UPDATE%'
) AS "blocked!"
