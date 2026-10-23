-- The Computer whose own Lore client presents a secret with this digest, and its owner.
-- veoveo: all tenants (a Computer's Lore client is addressed by its secret's digest, which names one Computer and its tenant)
SELECT id, tenant, principal_id FROM hub.computer WHERE lore_client_digest = $1
