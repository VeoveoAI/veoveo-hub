# Hub selection verification pilot — 2026-10-23

The pilot establishes that the Rust → Charon → Aeneas → Lean workflow is usable
for a small, production-linked Hub decision. The unchanged proof rejects a legal
Rust change that silently falls back from an unavailable explicit selection to an
available base. Restoring the Rust source restores the proof.

The agreed integration is selective, manually invoked verification. Develop Rust
normally, then explicitly choose when to extract the covered functions and check
their handwritten specifications in Lean. Start with this resolver; assess a
second decision before expanding. This experiment does not justify attempting to
translate or verify the entire Hub.

## Source and execution boundary

- Hub base: `3f2663d71fc5a87983d8a33a23c50a0d97aa25cb`.
- Historical pilot branch: `codex/lean-selection-pilot`, isolated worktree
  `/Users/rozgo/VeoveoAI/.worktrees/hub-lean-pilot`.
- Implementation: `/root/coder`, workspace role GPT-6.1 Sol.
- Tool setup and validation: `/root/builder`, workspace role GPT-6 Luna.
- Architecture, review and report: `/root`, workspace role GPT-6 Astra.
- Extraction and Lean checks ran on the existing Linux x86_64 host in an isolated
  user cache. They did not change the running Hub or the deployment.

The existing resolver was factored into a 57-line dependency-free
[Rust kernel](../../crates/veoveo-hub/src/execution/selection/kernel.rs).
The Hub adapter calls that kernel and maps its typed failures to the existing
errors. The proof crate imports the same source file through `#[path]`.
It is not a second implementation of the algorithm. Production retains its normal
compiler and dependency graph; extraction uses Charon's separately pinned compiler.

The kernel uses a slice/index loop and generic branch/deployment types. It
preserves the original missing, unique and ambiguous match outcomes. This report
records the pre-merge pilot qualification; it does not claim a runtime deployment
of the refactor. All source and proof links are relative to the repository and
remain usable after the original worktree is removed.

## What Lean checked

The [handwritten proof](SelectionProof.lean) contains three universal theorems
over the [generated Rust model](HubSelectionProof.lean):

1. For any served slice, comparator and explicit selection, changing the base
   cannot change the result. This includes modeled Rust failure/nontermination
   outcomes; it has no successful-lookup or equality-law premise.
2. If the generated lookup returns `Missing` for an explicit selection, resolution
   returns the typed `SelectedMissing` error.
3. That missing-selection result still holds when lookup reports a unique,
   available base deployment.

The latter two theorems assume the lookup classifications. They do **not** prove
that an absent branch is always classified as missing, scan termination, or the
complete correctness of the matching algorithm. No finite test enumeration or
handwritten replacement model is presented as a universal proof.

For all three successful theorem declarations, `#print axioms` reports exactly:

```text
[propext, Classical.choice, Quot.sound]
```

These are Lean's foundational axioms. The successful proof uses no `sorry`,
`sorryAx` or custom unproved assumptions about Hub.

## Measured execution

The first extraction took 16 seconds for Charon and 1 second for Aeneas. Aeneas
generated 18,003 bytes of Lean, including the complete matching loop and resolver.
A fresh extraction of the frozen proof package reproduced those bytes exactly.

The first `lake build` plus explicit `lake env lean SelectionProof.lean` completed
in 84 seconds. That includes fetching/decompressing 9,031 Mathlib cache artifacts
and building its cache helper. It is not a warm proof-check benchmark. Lake
reported 4.3 seconds for the generated module and 8.4 seconds for the handwritten
proof module during that run.

The semantic mutation experiment used the
[reviewed patch](fallback-mutant.patch), in a separate source snapshot:

1. Change the missing-explicit-selection arm to serve the base when the base has
   a unique deployment. `cargo check` succeeds.
2. Run fresh Charon and Aeneas extraction. Both succeed, and generated Lean has a
   different SHA-256 from the baseline. The generated Lean module itself compiles.
3. Check the **unchanged** proof. Lean exits with failure on both base-independence
   and missing-selection goals. This is a proof failure after successful
   compilation/extraction, not a syntax or unsupported-Rust failure.
4. Run the concrete Rust regression separately. With served `[(1, 42)]`, selected
   `Some(2)` and base `Some(1)`, the mutant returns `Ok(42)` rather than
   `Err(SelectedMissing(2))`; Cargo exits 101. This witnesses the actual semantic
   defect, rather than relying only on an old proof script failing.
5. Restore an independent baseline source snapshot and repeat Rust testing,
   extraction and proof checking. All pass. Generated Lean returns to the exact
   baseline hash; the handwritten proof never changes.

The restored run's cached `lake build` took 6 seconds and the explicit Lean check
took 2 seconds. These are whole-second wall measurements of one run, not a
statistical performance benchmark or the complete Rust-to-proof pipeline timing.

The failed mutant elaboration temporarily reports `sorryAx` in its error output:
Lean inserts recovery placeholders for failed declarations, and exits nonzero.
Those are failed-check diagnostics, not an accepted proof. Baseline and restored
outputs have only the three foundational axioms above.

The compact [qualification receipt](evidence/pilot-receipt.json) records the
historical frozen source snapshots, tool/binary pins, command environments, phase
exits, measured timings and raw-log hashes. Its SHA-256 is
`a89d14508dc9775e312491a429c6d7da0ac2bfc4ad29b908ec9464dbc08e6b55`.
Those snapshot hashes have not been rewritten to imply that later documentation
and artifact-hygiene edits participated in the recorded proof runs.

Raw baseline, mutant, restored and production-test logs are preserved locally at
`<workspace>/.cache/lean-pilot/evidence/`; the original workspace was
`/Users/rozgo/VeoveoAI`. They are deliberately excluded from Git, together with
tool caches, `.llbc`, build output and temporary snapshots. A fresh clone contains
the receipt and reproducible commands, not those machine-local logs.

The normal production adapter regression also passed on Linux, using Rust 1.99.0
in a disposable `veoveo-hub-check:e0-v5` container. The exact base Git archive plus
the final pilot overlay supplied the source; cached compiled dependencies and
SQLx offline metadata supplied the normal test environment:

```sh
cargo test --locked -p veoveo-hub --lib a_selection_routes_its_branch_and_never_falls_back
```

The command completed in 63 seconds: **1 passed, 240 filtered out**, with 0.04
seconds reported for the test body. The retained local log is
`hub-adapter-regression.log`, with its hash in the receipt. The first offline Cargo
attempt could not fetch the pinned OpenShell Git dependency missing from the
image cache; retrying with network access fetched it and completed successfully.
This was a cache/setup issue, not a failed Hub assertion.

The final standalone harness also passed formatting and its one Rust regression
test; the final Rust test command took 3.7 seconds. Production source formatting
and `git diff --check` passed. These recorded checks are component checks; the
historical receipt does not include a deployed probe, model-driven acceptance,
or full Mac/Linux release gate. Normal release gates remain separate.

## Reproducibility and byte identities

Exact tools and commands are in [README.md](README.md); the portable
[Lake manifest](lake-manifest.json) locks transitive dependencies as well.
The historical `implementation-final-v2.json` source manifest is preserved in the
same local evidence cache. The receipt records its hash and the frozen archive's
hash. It covers the 14 implementation/proof-package files from that qualification
snapshot, not subsequent report and package-hygiene edits.

- Aeneas: `fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf`.
- Charon: `de8736f7c995ac54dc490345cef9ea5143c06c62`.
- Extractor Rust: `nightly-2026-09-17`.
- Lean: `4.35.0`, commit `eb6cafb7290e8bd5f2937522d8d804f58f02bafc`.
- Mathlib: `56a005855ea17e0b0958f73ec801ae140b0a1679`.

SHA-256 identities:

```text
production kernel
3f0b53882fd043a3292184d3e3f71799de77058175fcd58ef51f82ce7abfe7c1
handwritten proof, unchanged in all three phases
dcbbb168a631c1c2ce44be69028a24239bbe6ac8d51b01f3ea384a8029ea1c23
generated Lean, baseline and restored (18,003 bytes)
949f66438af4be34f2c68ad5b83ab7ccf951538da60b2b436ec74866dad61951
generated Lean, mutant
01828614e3b57421de0be65bebeb1accd70af71b7535d5a52a41b064f94bfa84
mutation patch
67fd270edd8d84572d6a3260a547b030d61fef710cc360c85d4d48f49ecf8c28
Lake manifest
82ce65b4816bee7c38816026cb387ebb2460460c95cc8947bb9cf7843fb0e3b8
```

## Integration judgment

The useful result is a repeatable check tied to the Rust implementation, with a
measured negative control. The property applies to arbitrary modeled inputs,
where the existing unit test covers concrete examples. The unit test already
covered this particular bug; the pilot found no previously unknown Hub defect.
It demonstrated the verification workflow and its limits.

The user chose an on-demand proof loop, not CI. There is no automatic proof job,
file watcher, Git hook, or proof requirement for pushing, merging or deploying.
The explicit manual commands are in [README.md](README.md#regeneration-and-checking).
A developer can run those commands or ask the coding agent to verify a chosen
component when its behavior is ready to examine.

The workflow is:

1. Develop and iterate in Rust using the ordinary tests.
2. Explicitly select the component and properties to verify. Write or update the
   specifications when needed; Aeneas generates the code model, not the intended
   requirements or their proofs.
3. Regenerate Lean from the current Rust, check the proofs and their axiom
   dependencies, and investigate any failure before deciding whether the Rust,
   specification or proof needs changing.
4. Record the outcome against the source revision/content hashes used for that
   run. Keep the specifications and proofs alongside the code for the next
   manually requested verification.

An earlier successful proof remains evidence for its recorded source snapshot.
Development can continue without rerunning it; the next manual run establishes
whether the updated source satisfies the chosen properties. This is a reporting
boundary, not an additional runtime or development gate.

Use the cached Linux toolchain already qualified by this pilot. Native Mac Aeneas
execution was not qualified; Mac Rust development needs no mandatory local proof
installation. Toolchain upgrades and changes to the verification setup can be
qualified explicitly with the existing semantic mutation experiment.

Costs are real: a separate Rust compiler pin, coupled Charon/Aeneas/Lean versions,
generated files, specifications and proof maintenance. The first setup also
downloaded substantial tooling. A slow Mac-to-Linux compiler transfer was replaced
by a direct Linux download (13 seconds plus 3 seconds extraction); that delay is
not proof-check cost. This run did not measure long-term maintenance, a toolchain
upgrade, or throughput across many proofs.

One test-harness portability correction was made after the negative experiment:
the isolated source snapshot omitted the repository's root `rustfmt.toml`, which
changed grouped-import sorting. The standalone test now imports the `kernel`
module and qualifies its calls, so it formats consistently in both contexts.
The kernel, generated Lean and handwritten proof bytes are unchanged. Only the
final harness formatting/test checks were repeated for that change, and passed.

The selection-precedence theorem is particularly simple because the Rust branch
structure makes the invariant explicit. Passing it is evidence for selective use,
not evidence that complex lifecycle/concurrency proofs will be equally cheap.
Choose the next property from an existing, important decision in the code rather
than introducing a new runtime mechanism to make verification convenient.

No extra runtime authorization, service, rejection layer or orchestration is
needed for this approach. The remaining trust boundary includes Rust/Charon
extraction, Aeneas translation and library models, source linkage, and Lean's
kernel/foundations. The proof does not establish correctness of the compiled
machine code, HTTP/authentication, database transactions, registration gathering,
captured Task routing, deployment recovery, or Hub's complete architecture.
