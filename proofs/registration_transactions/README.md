# Registration transaction verification

This slice connects registration transaction reasoning to the actual store and
PostgreSQL behavior. Native regression evidence is the starting point; there is
no checked registration Lean theorem yet. Execution remains manual.

The first native probe uses the real `RegistrationStore::register` with a
fixture-only barrier before an old-metadata replica's insert. Replacement used
to delete the previous registrations before waiting on catalog DDL, leaving the
unfinished replica out of that deletion. Both calls could succeed while old and
new registrations and logins survived. Locking the ordinary deployment read
before comparison makes it participate in the replacement's serialization.

The [native receipt](evidence/native-replica-replacement.json) records the failing
original and passing fixed source. The unchanged regression passed all complete
row and role assertions after the fix; six existing registration integration
tests, formatting and strict library Clippy also passed. This is component
evidence, with no new Lean proof or deployment qualification.

The first planned theorem concerns one retained deployment under an already
admitted, fixed namespace/key: ordinary replica addition and changed-metadata or
instance replacement must have serial-equivalent committed effects. Prove
complete staged operations rather than assuming a correct effect at commit,
exact output rows/logins, and abort exclusion. Construct valid same-metadata
replica success, replacement success, and the former unlocked-read failure.

This first theorem excludes RFC 7592 per-client updates, concurrent removal and
housekeeping, namespace-key changes, external-environment preparation and
post-commit service audit/reload. These remain later obligations in HUB-REGISTER-01;
they are not silently assumed to share the same transaction or invariant.
PostgreSQL isolation, constraints and lock semantics, native metadata equality
and typed-store correspondence are explicit boundaries. Native tests and reviewed
SQL do not constitute a formal extraction of the asynchronous transaction code.

The [source review](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-registration-transaction-source-review.md)
records the operation boundaries and why global equality of all replica metadata
would contradict the existing per-client update semantics.
