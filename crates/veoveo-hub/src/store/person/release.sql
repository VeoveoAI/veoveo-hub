-- Another person of the tenant gives up the address a sign-in names.
UPDATE hub.person
SET email = NULL, updated_at = $4
WHERE tenant = $1 AND email = $2 AND principal <> $3
