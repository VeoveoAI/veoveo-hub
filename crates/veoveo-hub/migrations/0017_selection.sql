-- Development hard cut: an instance serves one branch of its repository, and selection
-- names branches. Reset instances instead of migrating them.
ALTER TABLE hub.instance
    ADD COLUMN branch text NOT NULL CHECK (octet_length(branch) BETWEEN 1 AND 1000);
COMMENT ON COLUMN hub.instance.branch IS
    'The Lore branch name the instance serves; selection routes a server to the instance of a branch.';

-- A principal's selection: the branch whose instance serves each server it names. A
-- server it does not name uses the instance of the installation's base branch.
CREATE TABLE hub.selection (
    principal text NOT NULL CHECK (principal <> ''),
    server text NOT NULL,
    branch text NOT NULL CHECK (octet_length(branch) BETWEEN 1 AND 1000),
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (principal, server)
);
COMMENT ON TABLE hub.selection IS
    'Retention: until the principal clears the selection of that server; explicit operator reset removes development state.';

-- Every replica tells the principal's open sessions that their catalog changed.
CREATE TRIGGER selection_change
    AFTER INSERT OR UPDATE OR DELETE ON hub.selection
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('principal');
