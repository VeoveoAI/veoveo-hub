-- veoveo: unbounded (the failed links of one branch, at most one per link at the top level of its revision)
SELECT link
FROM hub.lore_runner_failure
WHERE deployment = $1 AND repository = $2 AND branch = $3
ORDER BY link
