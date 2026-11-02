-- The slug of the identity row of one tenant, if a publication recorded it.
SELECT slug FROM identity.tenant WHERE id = $1
