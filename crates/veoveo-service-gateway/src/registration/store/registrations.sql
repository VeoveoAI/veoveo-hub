-- Every replica routes every registered server, so a reconciliation reads them all.
-- veoveo: unbounded (one row per registration of a running server process, which the installation's grace period bounds)
-- veoveo: scan (a reconciliation reads every registration of the installation)
SELECT
    client_id,
    server,
    metadata AS "metadata: Json<ClientMetadata>",
    software_statement,
    access_token_digest,
    issued_at
FROM gateway.registration
ORDER BY client_id
