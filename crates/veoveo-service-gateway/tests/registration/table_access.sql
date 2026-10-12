-- The owner of a server table and whether a login may write it.
SELECT
    pg_catalog.pg_get_userbyid(c.relowner)::text AS "owner!",
    pg_catalog.has_table_privilege($1, c.oid, 'INSERT') AS "insert!"
FROM pg_catalog.pg_class AS c
JOIN pg_catalog.pg_namespace AS n ON n.oid = c.relnamespace
WHERE n.nspname = $2 AND c.relname = $3
