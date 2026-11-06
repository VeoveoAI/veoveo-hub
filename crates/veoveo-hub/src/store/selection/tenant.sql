-- veoveo: unbounded (one row per server each person of the tenant selected a branch of)
SELECT selection.principal, selection.server, selection.branch
FROM hub.selection AS selection
JOIN hub.person AS person ON person.principal = selection.principal
WHERE person.tenant = $1
ORDER BY selection.principal, selection.server
