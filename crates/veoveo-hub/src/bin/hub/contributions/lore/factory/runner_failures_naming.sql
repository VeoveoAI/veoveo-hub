-- veoveo: unbounded (the links that wait for one linked repository, at most one per link that names it)
SELECT repository, branch, revision
FROM hub.lore_runner_failure
WHERE deployment = $1 AND linked_repository = $2
ORDER BY repository, branch, link
