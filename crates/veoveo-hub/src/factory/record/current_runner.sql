-- The complete current state of one runner, using the same projection as listings.
SELECT repository, branch, link AS "link!", linked_repository AS "linked_repository!",
    tracked_branch AS "tracked_branch!", tracked_branch_name AS "tracked_branch_name!", revision,
    pusher, linked_revision, linked_pusher
FROM hub.instance
WHERE repository = $1 AND branch = $2 AND link = $3
