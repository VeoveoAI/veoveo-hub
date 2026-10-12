SELECT EXISTS (
    SELECT FROM pg_catalog.pg_roles WHERE rolname = $1
) AS "exists!"
