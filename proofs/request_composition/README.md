# Request composition (manual)

This package connects the actual registration-selection and forwarded-request
assembly code to the authorization and recipient contracts tracked in
[Hub proof coverage](../README.md). It is an implementation-proof increment, not
a new runtime component or authorization rule. Proofs run manually.

## First checked assembly checkpoint

[SnapshotAssemblyProof.lean](SnapshotAssemblyProof.lean) checks six declarations:

- `ofWith_success_iff` and `ofWith_arbitrary_success`: snapshot assembly preserves
  its entire server map for arbitrary namespace-helper results; arbitrary successful
  namespace values are possible in the parameterized interface.
- `extracted_of_connection` and `extracted_of_success`: that interface is connected
  to the untouched extracted `of`; a successful actual result preserves the map.
  This does not prove the unresolved helper completes or chooses correct namespaces.
- `request_context_exact`: the actual function exactly constructs direct context,
  rejects missing direct execution, preserves delegated context, and rejects an
  explicitly conflicting delegated execution.
- `http_new_exact`: given successful context construction, the actual constructor
  returns exactly the independently specified complete client value. Recipient,
  actor, authority, supplied permissions, context, delegation and transport fields
  are preserved. This does not establish where the supplied registration or current
  permissions came from.

The [assembly receipt](evidence/assembly-checkpoint.json) records the exact nine
source/config files and Linux Lean 4.35.0 checks. Model compilation passed in
11.032 s, the direct proof check in 2.819 s and the default package build in 3.221 s.
The generated files retain their canonical extraction hashes.

Executable axiom audits reject unreviewed symbols and `sorryAx`. Universal/context
proofs permit only standard Lean axioms. The extraction connection permits the
explicit unresolved namespace helper. The HTTP theorem also permits the two native
error-formatting symbols referenced by the full constructor's error branch; its
accepted-branch proof assumes neither their success nor their contents. No issuer
result is allowed in that theorem's audit. Primitive-model/native correspondence
remains separate from these audits.

This is a checked proof checkpoint. Actual-source semantic controls, selected-route
composition and actual SDK issuance remain unfinished qualification work.

Run from this directory, using the committed toolchain and dependency manifest:

```sh
LAKE_NO_UPDATE=1 lake build
lake env lean SnapshotAssemblyProof.lean
```

## Intended contract

For an admitted request, the selected registration determines the recipient slug,
deployment and resource. The HTTP client preserves the admitted actor, authority,
permissions and request context. Issuance passes those exact fields to the actual
SDK issuer, with the existing delegation deadline cap. The issuer validates the
context, constructs the identity and claims, and the recipient checks its own
registered identity.

Selection must establish both refusal and useful acceptance: an unavailable,
missing or foreign-namespace selection cannot fall back to another deployment;
a present correctly selected deployment remains usable. Map-key/deployment
coherence and unique composition keys must be stated or derived, rather than
assuming the desired selected registration.

Direct request context is constructed from the subject's execution, audit, token
and original principal. A delegated request retains its previously admitted
context; an explicitly supplied conflicting execution is rejected. These are
different paths and must not be conflated in the forwarding contract.

The overall connected theorem is unfinished. Successful translation alone does
not establish any of these properties.

## Source and model ownership

`RequestCompositionHub/Types.lean` and `Funs.lean` are untouched Charon/Aeneas
outputs. The canonical extraction uses Hub
`3bc2923ca1d2e0c559e532286c16ec3d6a5ceb7a`, including runtime preparation
`91ab4cc0b63f7cc1950627df59f5ae3f1cb633e7`, and its actual SDK dependency
`239622d12c08b771cc5e813fe3e846385d8d04ad`. The existing catalog-field naming
annotation is extraction-only. Generated files are never patched to obtain a proof.
The [extraction receipt](evidence/extraction-checkpoint.json) records exact tool
arguments, input/output hashes and source-map checks. Its translation status is
separate from any subsequent Lean proof qualification.

`TypesExternal.lean` and `FunsExternal.lean` are handwritten operational models.
They reuse the existing current-role package's primitive representations where
appropriate. Their correspondence to native collections, identifiers, timestamps,
cloning and memory behavior remains an explicit boundary; a standard-axiom audit
does not establish that correspondence.

Two own-code boundaries need particular care:

- Namespace construction is separated from snapshot assembly. Any theorem about
  preserving the deployment map must hold for arbitrary helper results. It does
  not establish the helper's completion or ordinary namespace ordering.
- The SDK issuer is unresolved in the Hub extraction. It must be connected to the
  separately extracted actual SDK implementation through field-preserving typed
  adapters. An assumed issuer returning a desired identity is not a proof of
  forwarded identity or successful issuance.

Native SDK source-preparation commits after `239622d` are not part of Hub's current
dependency. Their tests and extraction cannot be claimed as proof of the pinned
stack without establishing and recording that integration.
SDK `e268032` successfully translates the actual issuer and context path after an
ordered-guard rewrite; its 35 native token tests pass. The
[translation record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-request-composition-translation.md)
states the exact inputs and limitations.

## Remaining scope

The full request-composition contract also requires current-policy/Task-route
linkage, actual SDK issuance and recipient checks, and explicit handling of each
error. Source-token expiry caps direct issuance; a valid delegated Task can outlive
that source token under its separate admitted deadline. Claims use JWT timestamp
seconds rather than preserving timestamp nanoseconds.

Cryptography and serialization, parser and URL/string primitives, async caller
wiring, store freshness, physical SQL transactions, deployment and recovery are
not established by translating or compiling this package. The
[native routing qualification](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-request-routing-native-qualification.md)
and [source checkpoint](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-30-request-composition-source-checkpoint.md)
are separate evidence, with their own inputs and limits.
