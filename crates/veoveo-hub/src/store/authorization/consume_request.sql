DELETE FROM hub.authorization_request
WHERE idp_state = $1 AND expires_at > $2
RETURNING
    idp_state, protected_resource, oauth_client_id, work_context, oidc_client,
    destination AS "destination: Json<AuthorizationDestination>", requested_scopes,
    idp_code_verifier, idp_code_challenge, idp_code_challenge_method, nonce,
    created_at, expires_at
