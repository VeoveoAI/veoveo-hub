-- The Hub applied the branch's own tree, without its links, to the runner's sandbox.
UPDATE hub.instance
SET own_tree = $2
WHERE deployment = $1 AND link IS NOT NULL
