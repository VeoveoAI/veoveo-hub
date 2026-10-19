SELECT draft #>> '{detail,revision}' AS "revision!"
FROM audit.record
WHERE class = 'hub.control_plane_publication'
ORDER BY sequence
