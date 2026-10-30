-- A single statement snapshot. Ciphertext is compared in memory, never Debug-printed.
SELECT jsonb_build_object(
    'families', COALESCE((SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id) FROM hub.refresh_family f), '[]'::jsonb),
    'tokens', COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.generation, t.id) FROM hub.refresh_token t), '[]'::jsonb),
    'audit', COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.sequence) FROM audit.record a), '[]'::jsonb)
) AS "snapshot!"
