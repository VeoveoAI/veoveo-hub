-- The table owner lifts the immutability trigger, as a database administrator could.
ALTER TABLE audit.record DISABLE TRIGGER record_immutable
