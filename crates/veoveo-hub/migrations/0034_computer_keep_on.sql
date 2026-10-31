-- An owner may keep their Computer on, so its own automations run while they are away:
-- collection skips it, and it stays kept on across restarts and updates until its owner
-- turns it off.
ALTER TABLE hub.computer ADD COLUMN keep_on boolean NOT NULL DEFAULT false;
COMMENT ON COLUMN hub.computer.keep_on IS
    'Whether the owner keeps the Computer on: collection never deletes its sandbox; only the owner changes it.';
