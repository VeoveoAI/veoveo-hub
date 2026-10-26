-- The person as this sign-in names them.
INSERT INTO hub.person (tenant, principal, display_name, email, updated_at)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (tenant, principal) DO UPDATE
SET display_name = EXCLUDED.display_name, email = EXCLUDED.email, updated_at = EXCLUDED.updated_at
