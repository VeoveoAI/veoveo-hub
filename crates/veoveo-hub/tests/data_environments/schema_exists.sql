SELECT EXISTS (SELECT FROM pg_catalog.pg_namespace WHERE nspname = $1) AS "exists!"
