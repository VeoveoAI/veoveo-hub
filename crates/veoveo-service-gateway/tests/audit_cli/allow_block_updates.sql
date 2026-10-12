-- The table owner lifts the immutability trigger, as a database administrator could.
ALTER TABLE audit.block DISABLE TRIGGER block_immutable
