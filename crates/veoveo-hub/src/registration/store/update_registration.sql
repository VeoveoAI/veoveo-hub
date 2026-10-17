UPDATE hub.registration
SET metadata = $2, software_statement = $3, updated_at = $4
WHERE client_id = $1
RETURNING client_id
