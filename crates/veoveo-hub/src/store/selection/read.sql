-- veoveo: unbounded (one row per server the principal selected a branch of)
SELECT server, branch
FROM hub.selection
WHERE principal = $1
