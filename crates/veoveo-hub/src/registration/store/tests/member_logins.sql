-- Bootstrap memberships of the fixture connection owner are shared server state,
-- not registration logins. Multiple grantors can repeat a membership.
-- veoveo: unbounded (a unit test enumerates login roles in its own unique fixture server group)
SELECT DISTINCT login.rolname::text AS "login!"
FROM pg_roles AS login
JOIN pg_auth_members AS membership ON membership.member = login.oid
JOIN pg_roles AS server ON server.oid = membership.roleid
WHERE server.rolname = $1 AND login.rolcanlogin AND login.rolname <> current_user
ORDER BY login.rolname::text
