SELECT jsonb_build_object(
    'deployment', (SELECT to_jsonb(d) FROM hub.deployment AS d WHERE d.id = $1),
    'claim', (SELECT to_jsonb(c) FROM hub.namespace_claim AS c WHERE c.server = $2),
    'registrations', (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.client_id), '[]'::jsonb)
                      FROM hub.registration AS r WHERE r.deployment = $1),
    'logins', (SELECT coalesce(jsonb_agg(jsonb_build_object(
        'name', p.rolname, 'login', p.rolcanlogin, 'superuser', p.rolsuper,
        'create_db', p.rolcreatedb, 'create_role', p.rolcreaterole,
        'replication', p.rolreplication, 'bypass_rls', p.rolbypassrls,
        'connection_limit', p.rolconnlimit, 'valid_until', p.rolvaliduntil,
        'memberships', (SELECT coalesce(jsonb_agg(g.rolname ORDER BY g.rolname), '[]'::jsonb)
            FROM pg_auth_members AS m JOIN pg_roles AS g ON g.oid = m.roleid
            WHERE m.member = p.oid)
    ) ORDER BY p.rolname), '[]'::jsonb)
    FROM pg_roles AS p WHERE p.rolname = ANY ($3::text[]))
) AS "snapshot!"
