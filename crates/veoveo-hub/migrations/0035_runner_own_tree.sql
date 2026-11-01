-- What a runner last applied of its branch beside the branch's revision: the SHA-256 of
-- the revision's own tree, without the links at its top level. A push of the branch whose
-- own tree is the same, such as one that adds or removes a link, and that names the same
-- tracked revision, changes nothing for the runner.
ALTER TABLE hub.instance
    ADD COLUMN own_tree text CHECK (own_tree ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT instance_own_tree CHECK (link IS NOT NULL OR own_tree IS NULL);
COMMENT ON COLUMN hub.instance.own_tree IS
    'For a runner, the SHA-256 of its branch revision''s own tree, without links, that the Hub last applied to its sandbox; none for an ordinary instance.';
