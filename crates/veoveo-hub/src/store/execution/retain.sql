INSERT INTO hub.execution_composition (revision, environment, canonical)
VALUES ($1, $2, $3)
ON CONFLICT (revision) DO NOTHING
