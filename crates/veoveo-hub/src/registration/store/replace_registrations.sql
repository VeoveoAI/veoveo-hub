-- A new registration of an instance removes the previous ones of its deployment. The
-- caller drops each removed registration's login in the same transaction.
-- veoveo: unbounded (the registrations of one deployment, which each registration of its instance replaces)
DELETE FROM hub.registration
WHERE deployment = $1
RETURNING client_id, server, login
