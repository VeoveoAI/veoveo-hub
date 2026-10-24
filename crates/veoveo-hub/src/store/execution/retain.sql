INSERT INTO hub.execution_composition (revision, canonical)
VALUES ($1, $2)
ON CONFLICT (revision) DO NOTHING
