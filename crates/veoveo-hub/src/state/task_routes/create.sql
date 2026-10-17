-- The unique source tuple arbitrates concurrent projections; the loser keeps the
-- winner's route.
INSERT INTO hub.task_route (
    id, tenant, owner, work_context, profile, server, source_task_id, authority_digest,
    owner_kind, owner_issuer, owner_subject, invocation_mode, initiator, delegation_id,
    data_labels, created_at, expires_at, delegated_caller
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
ON CONFLICT (server, source_task_id, owner, profile) DO NOTHING
