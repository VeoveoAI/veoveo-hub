DELETE FROM gateway.registration
WHERE client_id = $1
RETURNING client_id, server, login
