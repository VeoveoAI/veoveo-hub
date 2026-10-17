SELECT client_id, xmin::text AS "version!", orphaned_at
FROM hub.registration
ORDER BY client_id
