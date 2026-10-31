-- The Hub applied a revision of the runner's tracked branch to its sandbox.
UPDATE hub.instance
SET linked_revision = $2
WHERE deployment = $1 AND link IS NOT NULL
