SELECT client_id, xmin::text AS "version!", orphaned_at
FROM gateway.registration
ORDER BY client_id
