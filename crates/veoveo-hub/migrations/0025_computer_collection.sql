-- Collection deletes the sandbox of a Computer idle for `computers.collectAfterSeconds`,
-- keeping its home, as one operation; the next entry creates it at the current version.
ALTER TABLE hub.computer ADD COLUMN collected_at timestamptz;
COMMENT ON COLUMN hub.computer.collected_at IS
    'When collection last deleted the Computer''s idle sandbox, keeping its home; cleared when an operation creates one again.';
