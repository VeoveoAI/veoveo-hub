# Registration transaction verification

This slice connects registration transaction reasoning to the actual store and
PostgreSQL behavior. Native regression evidence, complete staged programs, lock
ownership, stable observations, exact write effects, serial-history composition
and abort-event exclusion are checked. Constructed valid replica admission and
completed same-metadata overlap are checked too; replacement/abort witnesses and
the original-protocol counterexample remain pending.
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

`RegistrationHistory.lean` composes these results. It derives the selected program
branch from the ordinary full-metadata comparison or unconditional instance
replacement, and preserves each attempt's original request. `committing_effect`
therefore proves that every reached commit applies the independent sequential
operation. `execution_serial` lifts this to every finite fixed-protocol execution,
in the order of actual server-commit events, while erasing only disjoint frame
changes. `aborted_event_excluded` excludes an aborted attempt's identity from
those events; `serial_history_and_abort` establishes both facts for the same
reached state. See the [serial-history receipt](evidence/serial-history.json).

`RegistrationWitnesses.lean` constructs valid initial rows/logins and actual
step-by-step executions. `seeded_replica_admitted` admits a replica while a second
attempt is waiting; `same_metadata_overlap_succeeds` completes both attempts, which
began before the first released its lock. Both acknowledge success in actual
commit order `[0, 1]`; the exact output equals two independent replica additions,
preserving the seed, both complete candidate rows/roles, unrelated login and frame.
Metadata, row payloads, role payloads and frame stay parametric; only mathematical
client/login representatives are fixed. See the
[valid-replica receipt](evidence/valid-replicas.json).

Next: construct replacement/abort executions and establish the original
unlocked-read failure. The earlier native test remains separate from these
design-model statements. Neither the history theorem nor a successful
acknowledgement implies that a registration survives a later replacement; the
constructed successful schedule is not an unconditional liveness proof.

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
