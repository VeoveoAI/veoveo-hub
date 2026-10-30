# Task continuity and recipient binding

This manual proof package shares the actual Rust Task decisions used by Hub and
the SDK's private forwarded-recipient check. It is not a proof of the durable
Task lifecycle, PostgreSQL, token cryptography or the deployed platform. The
[coverage map](../README.md) tracks the wider system property.

## Source boundary

The standalone Cargo crate imports these files with `#[path]`, without copying
their algorithms or resolving the full Hub/SDK dependency graphs:

- [Hub Task decisions](../../crates/veoveo-hub/src/state/task_routes/decision.rs):
  binding/ownership/label containment, delegated capture, retained capture and
  deployment agreement, successful request assembly and unchanged retry output.
- SDK `crates/veoveo-hub-sdk/src/internal_auth/recipient.rs`, in the exact sibling
  checkout `../veoveo-hub-sdk` relative to the Hub repository: server/resource
  agreement followed by deployment agreement, returning the matched recipient.

The implementation bases are Hub `651390dbbb8b4e65b5b8bcd3f8714a981fac2371`
and SDK `7e2489dccbee6d00b377b174f68f9d5de692a68b`. A qualification records the
exact changed source bytes from both repositories. At that qualification, Hub's SDK
dependency was `a41c9cc6c81f6c0305dd41165defb142740ed6b1`; the qualification did not
claim that this historical pin contained the private SDK refactor. No wire contract,
dependency pin or protected SDK hosting/Task implementation changes here.

## Current request-routing native coverage

Hub `eb099bb` pins SDK `239622d`. Its Task decision source and that pinned SDK's
recipient source match the qualified proof inputs byte for byte; the older dependency
caveat above does not describe this current pin. This source applicability check
does not extend the old theorem to the intervening routing and token assembly.

The [native routing regression](../../crates/veoveo-hub/tests/routing.rs) now verifies
recorded tokens with the actual recipient's registration keys and checks the complete
server/deployment/resource association and typed request context. After a Hub restart,
the same caller's retained Task keeps its deployment and composition while current
grants narrow from read/write to read and then are removed. Refusal is checked against
new **verified audit request IDs** at all four fixture endpoints; an old connection's
cleanup may continue under its earlier request context. Authentication timestamps are
fresh per request; only that field is normalized in the cross-request principal
comparison. This is a single-process component test against private PostgreSQL and
SDK fixtures, not a new Lean theorem, replica-convergence result or deployed test.

The [native receipt](evidence/request-routing-native-receipt.json) records source and
both test/Hub executable identities, the deliberate policy-bypass control and exact
restoration. Endpoint changes within the same captured deployment, full claims
assembly, live snapshot timing and other lifecycle operations remain separate work.

## Properties and assumptions

The checked statements establish soundness **and completeness**, with preserved output:

- Containment means every recorded label occurs among current labels. Order and
  duplicate entries do not change it. Comparisons must terminate and implement
  equality; an arbitrary Rust `PartialEq` implementation is not assumed lawful.
- Owner admission requires equal tenant/owner/Work Context/profile binding, equal
  actor identity and provenance, retained-label containment, and equal capture for
  delegated callers. It carries the original canonical/source IDs, server,
  deployment and capture, with the opaque **current** caller payload.
- Capture admission requires the retained composition context and its selected
  deployment to agree with that Task. Projection uses the same exact decision.
- Retry requires the full existing retry key and a strictly future expiry. It
  returns the original record, capture and expiry. Proposed composition is not
  part of the retry key; a different composition reaching the same deployment
  does not replace the original capture. Ordering must be total and correct for
  the supplied expiry representation; production retains exact `DateTime<Utc>`.
- Recipient admission requires the expected server/resource/deployment and returns
  the presented recipient unchanged. The joint statement relates it to the
  capture-admitted Task, rather than proving an unrelated recipient predicate.

Production adapter correspondence remains explicit. The Task binding uses the
existing deterministic identity projections. Ownership identity is the typed
principal/kind/issuer/subject/provenance tuple; adapters present borrowed label
references from actual sets. `with_task_execution` changes only the concrete
`AuthenticatedSubject.execution` field; its focused Rust regression compares the
whole current subject. Those dependency-heavy concrete projections are not extracted.

The label adapter creates two temporary `Vec`s of borrowed references and removes
the previous clone of the current ownership record. The shared scan costs
O(required labels × held labels), replacing the native `BTreeSet` subset operation;
this preserves the logical decision, not its asymptotic performance. No benchmark
or zero-overhead claim accompanies this slice.

Owner admission still precedes the composition read. The store must return the
requested retained composition and current registration snapshot. Actual current
profile exposure and policy run before successful canonical routing; the proof
does not accept an `authorized = true` input as a substitute. The selected catalog
must associate its registration's endpoint with the same captured deployment;
forwarding uses that registration and the admitted current subject. The formal
recipient composition assumes that exact endpoint/resource association and verified
decoded claims, not successful cryptographic verification or network delivery.

SQL uniqueness, concurrent insertion, expiry selection/pruning, current policy,
signature and issuer/time checks, field serialization and transport remain separate
obligations. Core owns claim/revision fencing, cancellation, recovery and provider
operations. Task-derived delegation's live-status and session reads remain in
Hub's `delegation.rs`, outside this extracted trace. A cancellation accepted by a
server is not necessarily terminal yet.
No cross-database/RPC atomicity or exactly-once external execution is claimed.

## Qualification

The [compact receipt](evidence/task-continuity-receipt.json) records the exact two
source snapshots, generated model, handwritten proofs, tools, commands, actual exits,
timings and retained-log hashes. Fresh baseline and restored runs pass all five Rust
witnesses, extraction, the default Lake build and both direct Lean checks. Target
axiom closures contain only `propext`, `Classical.choice` and `Quot.sound`.

Each of the four semantic mutants compiles as Rust, freshly extracts and builds its
generated model; its deterministic Rust witness and unchanged dispatch proof reject
it. Restoration recovers the original source and generated-model identities.
Focused production Rust, formatting and Clippy checks are listed by their actual
snapshot in the receipt; these do not qualify every SQL or asynchronous obligation.
No deployment or model-driven acceptance run belongs to this proof slice.

Earlier backend limitations, failed proof drafts and an incorrectly assembled
production test context remain recorded as diagnostic history. Their results are
not counted as successful qualification of the final source.

## Reproduce manually

Use the existing pinned toolchain: Aeneas
`fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf`, Charon
`de8736f7c995ac54dc490345cef9ea5143c06c62`, Rust
`nightly-2026-09-17`, and Lean `4.35.0`. `lake-manifest.json` pins the complete
Lean dependency set. Make those tools available on `PATH`; Charon's driver needs
the pinned Rust toolchain's `lib` on `LD_LIBRARY_PATH` on Linux.

Keep the exact SDK checkout beside the Hub checkout, including its recorded private
recipient source. From this directory:

```text
cd rust
cargo test --locked --lib
charon cargo --preset=aeneas
cd ..
aeneas -backend lean -dest . rust/hub_task_proof.llbc
lake build
lake env lean TaskContinuityProof.lean
lake env lean TaskDispatchProof.lean
```

`HubTaskProof.lean` is generated. Handwritten specifications and proofs belong in
`TaskContinuityProof.lean` and `TaskDispatchProof.lean`; never repair a proof by editing
generated output. The direct commands print the target theorem axiom closures.
Unused generated formatting/derive stubs are not accepted as target proof dependencies.
The joint theorem composes actual extracted functions; the asynchronous production
call sequence and endpoint/subject adapters remain reviewed and tested obligations.
The manual qualification must freshly extract a legal semantic mutant, check that the
unchanged semantic proof rejects it, then restore the original source/model and
rerun successfully. Keep compact receipts and source/model identities, with raw
logs and temporary snapshots in the workspace's `.cache/lean-pilot` evidence area.
No CI, hook, runner or mandatory gate is added.

## Semantic negative controls

Use a separate immutable copy for the baseline, each mutant, and restoration.
Each patch is rooted at the parent of the sibling repositories. From that parent,
`git apply veoveo-hub/proofs/task_continuity/mutations/NAME.patch` changes the actual
shared Rust source. Do not apply these patches to the development checkout.

| Patch | Standalone Rust witness filter | Formal boundary expected to reject it |
| --- | --- | --- |
| `capture-deployment` | `wrong_selected_capture_deployment_is_refused` | Capture agreement and composed dispatch |
| `retained-label` | `missing_retained_label_is_refused_for_an_otherwise_matching_owner` | Ownership and composed dispatch |
| `expiry-boundary` | `retry_preserves_original_record_and_refuses_expiry_boundary` | Strict expiry and unchanged retry admission |
| `recipient-deployment` | `recipient_refuses_other_deployment_in_the_same_namespace_and_resource` | SDK recipient binding and composed dispatch |

For each mutant, first run `cargo check --locked --lib`, fresh Charon/Aeneas
extraction, and `lake build HubTaskProof`; these must succeed. Then direct-check
the unchanged handwritten proofs and run `cargo test --locked --lib FILTER`
separately, preserving both expected failures. A Rust witness failure must not
prevent extraction or the independent Lean check. Unused-variable warnings caused
by intentionally deleting a comparison are not a Rust compilation failure.

Restore the original source in another copy, regenerate, and repeat the positive
commands and axiom inspection. Restoration must recover the recorded source and
generated model hashes. Raw diagnostic logs, including failed proof drafts and
misassembled attempts, remain private workspace evidence; the compact receipt
records the accepted snapshot and actual outcomes without rewriting that history.
