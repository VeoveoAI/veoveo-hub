BEGIN TRANSACTION;
LET $head = (SELECT * FROM ONLY gateway_control_active:current FOR UPDATE);
IF $head.revision != $expected { THROW 'control_plane_revision_changed'; };
CREATE ONLY $revision_record CONTENT $revision;
FOR $object IN $objects {
    CREATE gateway_control_object CONTENT $object;
};
DELETE work_context WHERE id NOT IN $work_context_ids;
FOR $context IN $work_contexts {
    UPSERT $context.id MERGE {
        tenant: $context.tenant,
        context_key: $context.context_key,
        title: $context.title,
        policy_revision: $context.policy_revision,
        output_policy: $context.output_policy,
        memberships: $context.memberships,
        updated_at: $context.updated_at
    };
};
UPSERT ONLY gateway_control_active:current CONTENT {
    revision: $revision_record,
    revision_id: $revision_id,
    updated_at: $applied_at
};
fn::audit::append($audit_rows);
COMMIT TRANSACTION;
