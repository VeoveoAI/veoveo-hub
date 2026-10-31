-- Every replica routes every registered server, so a reconciliation reads them all.
-- veoveo: unbounded (one row per registration of a running server process, which the installation's grace period bounds)
SELECT
    registration.client_id,
    registration.server,
    registration.metadata AS "metadata: Json<ClientMetadata>",
    registration.software_statement,
    registration.access_token_digest,
    registration.issued_at,
    registration.orphaned_at,
    instance.branch AS "branch?",
    instance.link AS "link?",
    instance.linked_repository AS "linked_repository?"
FROM hub.registration
LEFT JOIN hub.instance ON instance.deployment = registration.deployment
ORDER BY registration.client_id
