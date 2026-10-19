-- Synthetic delivery envelopes exercise more than one native clearing batch.
-- No credential is issued or decrypted by this fixture.
INSERT INTO hub.refresh_token (
    id, family, token_digest, generation, issued_at, expires_at,
    delivery_envelope, delivery_expires_at
)
SELECT token.id, $2, 'sha256:' || lpad(to_hex(token.generation), 64, '0'),
       token.generation, $3, $4, 'synthetic-delivery-fixture', $4
FROM unnest($1::uuid[]) WITH ORDINALITY AS token(id, generation)
