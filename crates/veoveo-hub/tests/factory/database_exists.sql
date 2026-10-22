SELECT EXISTS (SELECT FROM pg_catalog.pg_database WHERE datname = $1) AS "exists!"
