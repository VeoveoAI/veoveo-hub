-- Executable native activation contribution: only exact resume identity/template
-- provenance and retained per-build listener placement, not native inventory/status.
CREATE TABLE hub.activation_runtime (
    name text PRIMARY KEY,
    identity jsonb NOT NULL CHECK(jsonb_typeof(identity)='object')
);
CREATE TABLE hub.activation_worker (
    build uuid PRIMARY KEY REFERENCES hub.activation_build(id),
    runtime text NOT NULL REFERENCES hub.activation_runtime(name),
    port integer NOT NULL CHECK(port BETWEEN 30000 AND 59999),
    placement jsonb NOT NULL CHECK(jsonb_typeof(placement)='object'),
    UNIQUE(runtime,port)
);
