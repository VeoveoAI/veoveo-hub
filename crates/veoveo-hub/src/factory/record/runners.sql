-- veoveo: unbounded (the runners of one branch, one per link at the top level of its revision)
SELECT repository, branch, link AS "link!", linked_repository AS "linked_repository!",
    tracked_branch AS "tracked_branch!", tracked_branch_name AS "tracked_branch_name!", revision,
    pusher, linked_revision, linked_pusher
FROM hub.instance
WHERE repository = $1 AND branch = $2 AND link IS NOT NULL
ORDER BY link
