-- Named environments are gone: a request routes each slug to its newest registered
-- deployment, or by branch. Development hard cut: retained compositions name an
-- environment in their version 1 canonical record, which no longer reads, so they and
-- the Task routes that keep them are removed rather than rewritten.
DELETE FROM hub.task_route;
DELETE FROM hub.execution_composition;
ALTER TABLE hub.task_route DROP COLUMN environment;
ALTER TABLE hub.execution_composition DROP COLUMN environment;
COMMENT ON TABLE hub.execution_composition IS
    'Retention: immutable execution captures remain while retained operations refer to them; explicit operator reset removes development state.';
