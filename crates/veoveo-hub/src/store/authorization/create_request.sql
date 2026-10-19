INSERT INTO hub.authorization_request (
    idp_state, protected_resource, oauth_client_id, work_context, oidc_client,
    destination, requested_scopes, idp_code_verifier, idp_code_challenge,
    idp_code_challenge_method, nonce, created_at, expires_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
