-- A branch of a server repository whose revision holds links at its top level is served
-- by one runner per link: an instance of the branch that also names its link, the linked
-- repository and the branch of it that the link tracks, with that branch's latest pusher
-- and the revision of it the Hub last applied. An ordinary instance names none of them.
ALTER TABLE hub.instance
    ADD COLUMN link text CHECK (link ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'),
    ADD COLUMN linked_repository text CHECK (linked_repository ~ '^[0-9a-f]{32}$'),
    ADD COLUMN tracked_branch text CHECK (tracked_branch ~ '^[0-9a-f]{32}$'),
    ADD COLUMN tracked_branch_name text
        CHECK (octet_length(tracked_branch_name) BETWEEN 1 AND 1000),
    ADD COLUMN linked_pusher text CHECK (linked_pusher <> ''),
    ADD COLUMN linked_revision text CHECK (linked_revision ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT instance_runner CHECK (
        (link IS NULL) = (linked_repository IS NULL)
        AND (link IS NULL) = (tracked_branch IS NULL)
        AND (link IS NULL) = (tracked_branch_name IS NULL)
        AND (link IS NOT NULL OR (linked_pusher IS NULL AND linked_revision IS NULL))
    ),
    DROP CONSTRAINT instance_repository_branch,
    ADD CONSTRAINT instance_repository_branch_link UNIQUE NULLS NOT DISTINCT (repository, branch, link);
CREATE INDEX instance_tracked ON hub.instance (linked_repository, tracked_branch)
    WHERE linked_repository IS NOT NULL;
COMMENT ON COLUMN hub.instance.link IS
    'The name of the link at the top level of the branch''s revision that a runner serves; none for an ordinary instance.';
COMMENT ON COLUMN hub.instance.linked_repository IS
    'The Lore repository ID the runner''s link names, whose tracked branch the runner serves.';
COMMENT ON COLUMN hub.instance.tracked_branch IS
    'The ID of the linked repository''s branch the link tracks: the parent branch''s ID for a link that follows it, else the branch of the link''s pinned revision.';
COMMENT ON COLUMN hub.instance.tracked_branch_name IS
    'The tracked branch''s name as the Hub last read it, which the runner''s status names so people know where to push.';
COMMENT ON COLUMN hub.instance.linked_pusher IS
    'The latest pusher of the tracked branch, whose Lore token the runner''s client issues for the linked repository.';
COMMENT ON COLUMN hub.instance.linked_revision IS
    'The revision of the tracked branch the Hub last applied to the runner''s sandbox.';

-- Why the Hub could not run a link of a branch: nobody watches the linked repository, the
-- watcher's grant was refused, or reading the tracked branch failed. A runner that exists
-- keeps serving meanwhile; a link without one gets none until this clears.
CREATE TABLE hub.lore_runner_failure (
    deployment text NOT NULL CHECK (deployment <> ''),
    repository text NOT NULL CHECK (repository ~ '^[0-9a-f]{32}$'),
    branch text NOT NULL CHECK (octet_length(branch) BETWEEN 1 AND 1000),
    link text NOT NULL CHECK (link ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'),
    linked_repository text NOT NULL CHECK (linked_repository ~ '^[0-9a-f]{32}$'),
    revision text NOT NULL CHECK (revision ~ '^[0-9a-f]{64}$'),
    message text NOT NULL,
    failed_at timestamptz NOT NULL,
    PRIMARY KEY (deployment, repository, branch, link)
);
COMMENT ON TABLE hub.lore_runner_failure IS
    'Retention: until the link''s runner is next applied or the link leaves its branch; explicit operator reset removes development state.';
CREATE INDEX lore_runner_failure_linked ON hub.lore_runner_failure (deployment, linked_repository);
