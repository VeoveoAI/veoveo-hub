INSERT INTO hub.activation_runtime(name,identity) VALUES($1,$2) ON CONFLICT(name) DO NOTHING
