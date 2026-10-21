INSERT INTO hub.activation_source (build,document) VALUES ($1,$2)
ON CONFLICT (build) DO NOTHING
