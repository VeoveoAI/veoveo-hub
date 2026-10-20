-- Test-owned admitted document; exercises the immutable publication marker only.
INSERT INTO hub.control_revision (id, digest, source, applied_at, applied_by, tenant, control_plane)
VALUES ($1, 'sha256:0000000000000000000000000000000000000000000000000000000000000000',
    'contribution', clock_timestamp(), $2, $3, $4)
