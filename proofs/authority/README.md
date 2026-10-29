# Delegated authority

This manual package covers Hub's ordered exchange decisions, successful grant
assembly and downstream delegated policy assembly. It is a partial boundary of
HUB-AUTH-01, not a proof of authentication, role evaluation or the complete token
exchange endpoint. The shared production source is
[`delegation/authority.rs`](../../crates/veoveo-hub/src/delegation/authority.rs),
imported directly by the standalone Rust crate with `#[path]`.

## Source and assumptions

The implementation basis is Hub `ee0bdf3b4f800ee31ed0e3017407a646244fd023`
plus the exact recorded source overlay. No SDK, wire, dependency, SQL or schema
change belongs to this slice. The recorded qualification applies to those bytes;
it does not automatically qualify subsequent code or group/principal changes.

Production calls `declare`, `address`, `provide` and `grant` in that order from
`DelegationService::issue`. JSON parsing precedes these stages; profile lookup,
resource parsing and current provider discovery occur between their respective
stages. `HubMcp::policy_decision` uses `narrow` after current policy and managed
client admission, keeping its original lazy declared-route lookup. The caller
sequence is concrete Rust reviewed and tested against the recorded snapshot; a
Lean composition of extracted calls is not an extraction of the entire async
endpoint.

Native adapters retain `BTreeSet` membership/filtering and the original streaming
provider grants filtered by the request before collection. There is no projection
to quadratic list scans or collection of all catalog permissions. Their finite-set
semantics are explicit operational proof premises: lawful membership, emptiness,
first missing use and exact intersection/filter results. This does not prove the
standard library or current role/grant evaluation. The resource adapter compares
the request's raw string exactly and carries the already validated typed resource;
it does not normalize or parse the request again.

The output retains the exact profile, typed resource, original caller, immediate
actor, Task and deadline. Deadline preservation here means preservation of an
opaque value. SDK `issue_exchanged` still owns inherited-deadline clamping,
whole-second JWT time admission, chain validation and signing. Request subjects
and live Task subjects retain their existing distinct lifetime rules. A new
exchange can request a different downstream namespace; no theorem assumes its
permission names are a subset of an earlier upstream namespace.

Downstream assembly intersects current grants with carried authority. Declared
`Authenticated` routes remain valid with zero permissions; a permission route
checks its carried permission, a server/Task target requires nonempty narrowed
authority, and an undeclared target is denied. An already denied decision keeps
its grant payload; managed denial only changes effect/reason. Opaque metadata is
preserved. Proving the required permission survives the final intersection also
requires consistency of the original current-policy decision: an arbitrary
Allowed input is not assumed to contain its route's permission.

Principal/group projection, catalog selection/freshness, current role/policy and
managed-client decisions, provider discovery, Task/session reads, client/assertion
validation, replay, cryptography, concrete SDK/token serialization and transport
remain outside the formal boundary. No new permission or rejection is introduced.

## Manual workflow

Use Aeneas `fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf`, Charon
`de8736f7c995ac54dc490345cef9ea5143c06c62`, Rust
`nightly-2026-09-17` and Lean `4.35.0`. The portable `lake-manifest.json`
pins Lean dependencies. Charon's driver needs the pinned Rust toolchain's `lib`
on `LD_LIBRARY_PATH` on Linux. From this directory:

```text
cd rust
cargo test --locked --lib
charon cargo --preset=aeneas
cd ..
aeneas -backend lean -dest . rust/hub_authority_proof.llbc
lake build
lake env lean AuthorityProof.lean
lake env lean AuthorityComposition.lean
lake env lean AuthorityInterpretation.lean
```

`HubAuthorityProof.lean` is generated; handwritten statements belong in
`AuthorityProof.lean`, `AuthorityComposition.lean` and `AuthorityInterpretation.lean`.
The default build covers all three; direct checks print target axiom closures. Never edit generated code to repair a theorem. Final
qualification records exact source/model/proof identities, axiom closures, fresh
legal semantic mutants and restored success. No CI, hook, runner or mandatory
verification gate is added. Raw logs and temporary snapshots stay in the shared
`.cache/lean-pilot` evidence area; compact receipts belong in this package.

## Semantic negative controls

The standalone witnesses use a small concrete bitset; production regressions use
native typed permissions and `BTreeSet` adapters. The Lean finite interpretation
separately shows the operational premises are satisfiable and includes accepted
nonempty filtering and accepted empty `Authenticated` output. None of these is a
proof of `BTreeSet` internals.

Use one immutable sibling-root snapshot per mutation, applying a patch from the
parent of `veoveo-hub` with `patch -p1`. Freshly run Rust check, Charon, Aeneas and
the generated-model build before the unchanged handwritten proofs. The semantic
proof and the named witness must fail; a compiler/extractor failure is not a
successful negative control. Run the concrete witness separately, so its expected
failure does not prevent formal qualification. Restore to a separate baseline
snapshot and recover exact source/generated/proof identities and all passes.

| Mutation | Concrete witness |
|---|---|
| `unfiltered-grant.patch` | `tests::exchange_keeps_current_authority_and_every_original_context_field` |
| `skip-downstream-filter.patch` | `tests::carried_authority_narrows_permissions_without_replacing_decision_metadata` |
| `reject-authenticated-empty.patch` | `tests::authenticated_empty_is_valid_but_undeclared_and_empty_server_are_refused` |
| `wrong-resource.patch` | `tests::wrong_resource_and_missing_provider_do_not_issue_a_grant` |

The grant mutant still invokes the current filter before returning the unfiltered
request, and the resource mutant still invokes the comparator before bypassing its
result. Their generic operational interfaces remain present; the intended failure
is the behavioral theorem, not an extraction/signature mismatch. A deliberately
failed proof elaboration may print `sorryAx`; it is negative diagnostic output,
never accepted proof evidence.

## Qualification

The [structured receipt](evidence/authority-receipt.json) records the exact historical
20-file source/proof freeze, toolchain, generated model, individual stage exits and
timings, retained-log hashes and superseded diagnostics. Final reporting prose and
the copied receipt do not change that measured freeze's identities.

| Independent run | Rust witnesses | Charon / Aeneas | Default Lake | Three direct Lean checks |
|---|---|---|---|---|
| Baseline | 6 pass, 180 ms | 190 / 798 ms | 10,522 ms | 3,459 / 2,977 / 2,973 ms |
| Restored | 6 pass, 169 ms | 179 / 704 ms | 10,490 ms | 3,543 / 2,807 / 2,778 ms |

These runs use cached pinned dependencies, not a complete Lean dependency rebuild.
The target axiom closures contain only `propext`, `Classical.choice` and `Quot.sound`.
The restored source and generated model recover the original hashes. All four
mutants compile, freshly extract and build their generated module, then fail the
unchanged semantic proof and the separate named witness at the intended behavior.

Focused native checks cover the ordered-set/resource adapters (2 tests), complete
policy-decision adapter (5), existing role grants (5) and exchange integration (1).
The grant/exchange tests passed on production2. Its strict Clippy failure was a
nested conditional; production3's equivalent early-return shape passed formatting,
the affected adapter tests and strict library Clippy. The receipt separates those
snapshots. An earlier fixture temporary-borrow compile error and failed Lean drafts
are retained as diagnostics, not accepted qualification. No SDK change, full platform
gate, deployment or application-model call belongs to this slice.

The source changes retain native ordered-set membership and the original filtered
provider stream. No quadratic collection projection was added. There is no benchmark
or general zero-overhead claim. The result remains partial system coverage: actual
role/override evaluation, declaration/parser correspondence and the asynchronous
caller/token/cryptographic boundaries still require their own evidence.
