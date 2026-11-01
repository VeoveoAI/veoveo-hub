-- The own tree of the branch and the tracked branch's revision that a runner last applied.
SELECT own_tree, linked_revision
FROM hub.instance
WHERE deployment = $1 AND link IS NOT NULL
