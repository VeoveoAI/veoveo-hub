-- A registration that replaces its deployment's binding removes the deployment's
-- previous registrations, and removing an instance removes all of them. The caller
-- drops each removed registration's login in the same transaction.
-- veoveo: unbounded (the registrations of one deployment, which a registration with new metadata replaces)
DELETE FROM hub.registration
WHERE deployment = $1
RETURNING client_id, server, login
