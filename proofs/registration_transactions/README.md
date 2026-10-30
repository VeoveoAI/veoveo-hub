# Registration transaction verification

This slice connects registration transaction reasoning to the actual store and
PostgreSQL behavior. Native regression evidence, complete staged programs, lock
ownership, stable observations and exact write effects are checked. Serial-history
composition and reachable valid-output executions remain pending.
Execution remains manual: run `lake build` in this directory with the pinned
Lean toolchain.

The first native probe uses the real `RegistrationStore::register` with a
fixture-only barrier before an old-metadata replica's insert. Replacement used
to delete the previous registrations before waiting on catalog DDL, leaving the
unfinished replica out of that deletion. Both calls could succeed while old and
new registrations and logins survived. Locking the ordinary deployment read
before comparison makes it participate in the replacement's serialization.

The [native receipt](evidence/native-replica-replacement.json) records the failing
original and passing fixed source. The unchanged regression passed all complete
row and role assertions after the fix; six existing registration integration
tests, formatting and strict library Clippy also passed. Those checks establish
native component behavior; the Lean result below is separate. No deployment
qualification ran.

`RegistrationModel.lean` represents one retained deployment under an already
admitted, fixed namespace/key, with parametric complete metadata, row and role
payloads. Its DELETE captures the exact finite returned rows at statement time;
commit applies only those staged key deltas. Ordinary fixed-source reads and
both fixed/original replacement upserts take the deployment lock. A server commit
request can resolve to commit or abort, independently of client delivery.

`RegistrationTransaction.committing_program_complete` in `RegistrationProof.lean`
derives complete staged operations for every reachable
committing attempt from an initial state and any finite modeled interleaving.
The loop invariant accounts for every captured login removal before candidate
admission. Primitive commit has no application-correctness guard. The audited
theorem uses only `propext` and `Quot.sound`; see the
[complete-program receipt](evidence/program-completeness.json).

`RegistrationLocks.lean` derives exclusive ownership for active fixed-protocol
attempts. `RegistrationObservations.lean` uses that ownership and program
alignment to prove that ordinary metadata observations and replacement's captured
rows still describe the current committed database throughout its active phases,
including commit-pending. Completed and abort-pending phases are excluded.

`RegistrationEffects.lean` equates complete staged programs with the independent
replica and replacement specifications, including complete candidate payloads,
old registration/login removal and unrelated-login preservation. Replacement's
algebraic theorem takes a current exact row listing as a premise; the observation
theorem establishes that premise for reachable active replacement attempts.
See the [lock, observation and effect receipt](evidence/locked-observations-effects.json).

These results are ingredients of serialization, not yet a proof of the complete
history. `RegistrationSpecification.lean` still has unproved serial-history and
abort obligations indexed by actual commit events. Next: compose those histories,
construct exact valid same-metadata and replacement executions, and establish the
original unlocked-read failure. The earlier native test remains separate from
these design-model statements.

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
