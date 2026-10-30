SELECT pg_backend_pid() AS "pid!",
       current_setting('transaction_isolation') AS "isolation!"
