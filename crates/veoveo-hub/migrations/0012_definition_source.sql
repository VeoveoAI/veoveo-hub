-- Executable-native association to the ordinary environment definition file.
CREATE TABLE hub.environment_definition_source (
    environment uuid PRIMARY KEY,
    owner_tenant text NOT NULL CHECK(owner_tenant<>''),
    owner_principal text NOT NULL CHECK(owner_principal<>''),
    generation bigint NOT NULL CHECK(generation>0),
    document jsonb NOT NULL CHECK(jsonb_typeof(document)='object'),
    CHECK((document->>'environment')::uuid=environment),
    CHECK((document->>'generation')::bigint=generation)
);
COMMENT ON TABLE hub.environment_definition_source IS 'Retention: current owner definition-source selector and last authoritative native observation survive restart. Local apply invalidates older Follow generations; no native bearer or event ledger is retained.';
