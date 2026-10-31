-- A Computer's Lore client, as a handoff stores it.
UPDATE hub.computer SET lore_client_digest = $3 WHERE id = $1 AND tenant = $2
