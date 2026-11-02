-- One statement snapshot, not a claim that successive ordinary SELECTs agree.
-- No sequence/identifier allocation state is projected here.
-- veoveo: all tenants (a unit test snapshots every control-plane row of its own fixture database)
-- veoveo: scan (a unit test reads every row of its own fixture database to compare snapshots)
SELECT jsonb_build_object(
    'active', (SELECT revision FROM hub.control_active),
    'revisions', COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM hub.control_revision r), '[]'::jsonb),
    'objects', COALESCE((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.revision, o.kind, o.object_id) FROM hub.control_object o), '[]'::jsonb),
    'enterprises', COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM identity.enterprise e), '[]'::jsonb),
    'tenants', COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM identity.tenant t), '[]'::jsonb),
    'audit', COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.sequence) FROM audit.record a), '[]'::jsonb)
) AS "snapshot!"
