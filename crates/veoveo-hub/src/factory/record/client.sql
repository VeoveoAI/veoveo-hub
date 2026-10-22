-- The instance's client gets a new secret with each new sandbox.
UPDATE hub.instance
SET client_digest = $2
WHERE deployment = $1
