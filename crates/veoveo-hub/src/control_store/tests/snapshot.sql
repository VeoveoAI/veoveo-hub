-- One statement snapshot, not a claim that successive ordinary SELECTs agree.
-- No sequence/identifier allocation state is projected here.
SELECT jsonb_build_object(
    'active', (SELECT revision FROM hub.control_active),
    'revisions', COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM hub.control_revision r), '[]'::jsonb),
    'objects', COALESCE((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.revision, o.kind, o.object_id) FROM hub.control_object o), '[]'::jsonb),
    'enterprises', COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM identity.enterprise e), '[]'::jsonb),
    'tenants', COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM identity.tenant t), '[]'::jsonb),
    'contexts', COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.context_key) FROM identity.work_context c), '[]'::jsonb),
    'grants', COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.work_context, g.ordinal) FROM identity.work_context_grant g), '[]'::jsonb),
    'memberships', COALESCE((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.work_context, m.ordinal) FROM identity.work_context_membership m), '[]'::jsonb),
    'audit', COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.sequence) FROM audit.record a), '[]'::jsonb)
) AS "snapshot!"
