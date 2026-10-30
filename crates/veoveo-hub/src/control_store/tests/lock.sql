-- An autocommit locker cannot complete before the dropped writer releases its row lock.
SELECT revision FROM hub.control_active FOR UPDATE
