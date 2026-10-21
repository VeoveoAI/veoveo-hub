INSERT INTO hub.environment_definition_source(environment,owner_tenant,owner_principal,generation,document)
VALUES($1,$2,$3,$4,$5)
ON CONFLICT(environment) DO UPDATE SET generation=EXCLUDED.generation,document=EXCLUDED.document
WHERE environment_definition_source.owner_tenant=EXCLUDED.owner_tenant
    AND environment_definition_source.owner_principal=EXCLUDED.owner_principal
