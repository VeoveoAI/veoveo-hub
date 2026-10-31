-- An administrator ends every session of a person: their families are revoked with a
-- reason of their own, beside the client's revocation and a replay.
ALTER TABLE hub.refresh_family DROP CONSTRAINT refresh_family_revocation_reason_check;
ALTER TABLE hub.refresh_family ADD CONSTRAINT refresh_family_revocation_reason_check
    CHECK (revocation_reason IN ('client_revocation', 'token_replay', 'administrator_revocation'));
