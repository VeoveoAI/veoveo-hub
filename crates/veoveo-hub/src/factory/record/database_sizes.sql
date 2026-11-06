SELECT datname::text AS "name!", pg_database_size(datname) AS "bytes!"
FROM pg_database
WHERE datname::text = ANY($1)
