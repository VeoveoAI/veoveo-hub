SELECT
    r.id,
    r.tenant AS "tenant: TenantId",
    r.owner AS "owner: PrincipalId",
    r.work_context AS "work_context: WorkContextId",
    r.profile,
    r.server,
    r.source_task_id,
    r.authority_digest,
    p.principal_key AS owner_key,
    r.owner_kind AS "owner_kind: PrincipalKind",
    r.owner_issuer,
    r.owner_subject,
    r.invocation_mode,
    r.initiator,
    r.delegation_id,
    r.data_labels,
    r.expires_at
FROM gateway.task_route AS r
JOIN identity.principal AS p ON p.id = r.owner
WHERE r.id = $1 AND r.expires_at > $2
