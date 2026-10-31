-- veoveo: unbounded (the runners whose links name one linked repository, one per link that names it)
SELECT repository, branch, link AS "link!", linked_repository AS "linked_repository!",
    tracked_branch AS "tracked_branch!", tracked_branch_name AS "tracked_branch_name!", revision,
    pusher, linked_revision, linked_pusher
FROM hub.instance
WHERE linked_repository = $1
ORDER BY repository, branch, link
