# Lookup correctness slice — 2026-10-23

This manually requested slice extends the original [pilot](RESULTS.md). It changes
no production Rust and does not introduce a CI job, hook, watcher or proof gate.
The original report and qualification receipt remain historical and unchanged.
Implementation and proof: `coder`, GPT-6.1 Sol. Focused Linux validation: `builder`,
GPT-6 Luna. Architecture/review: `root`, GPT-6 Astra.

## Source and assumptions

- Hub source base: `9ea2a657e622c9e2f8aad0c545204dc5a83b6a24`.
- Production kernel SHA-256:
  `3f0b53882fd043a3292184d3e3f71799de77058175fcd58ef51f82ce7abfe7c1`.
- Generated model SHA-256:
  `949f66438af4be34f2c68ad5b83ab7ccf951538da60b2b436ec74866dad61951`.
- The original handwritten `SelectionProof.lean`, generated model, compiler/tool
  pins and transitive dependency lock are unchanged.

The new [LookupProof.lean](LookupProof.lean) assumes `CorrectComparator`: for every
pair of branch values, the modeled comparator terminates successfully and returns
the Boolean decision of mathematical equality. Rust `PartialEq` alone does not
promise this. The theorem does not verify the concrete branch type's comparator;
no runtime validation or authorization rule was added to enforce the premise.
Deployment values need no equality decision and are not assumed distinct.

The specification is a mathematical matching-entry list followed by a zero/one/
at-least-two classification. The proof refines the unchanged generated loop into
that specification. Its recursive measure is the remaining suffix length, with
the starting-index bound carried through each successful increment. The top-level
lookup begins at zero, so it obtains the required bound for every modeled slice.
The proof uses Aeneas's total correctness specification, excluding modeled panic,
index/overflow failure and nontermination under the comparator premise.

## Coverage

The total result is Missing iff no entries match, Unique iff exactly one entry
matches with that deployment, and Ambiguous iff at least two entries match. This
counts entries rather than distinct deployment IDs. An explicit theorem and a
standalone Rust regression cover two identical branch/deployment entries.

The end-to-end absent-selection theorem has no assumed lookup result. It combines
the original universal base-independence theorem with the new total lookup
refinement, proving that any actually absent explicit branch returns the typed
SelectedMissing result for every base choice.

This does not verify registration gathering, real branch comparison code, HTTP,
authorization, SQL, OpenShell, machine code or deployed behavior. It is not a new
whole-Hub verification claim.

## Qualification and maintenance

Focused proof development used the existing pinned Linux cache. The total loop
refinement first checked successfully in iteration 5; the complete classification
and resolver theorem set checked in iteration 6. That direct check took 3.07
seconds, and the subsequent cached `lake build` took 3.63 seconds. These are single
run wall measurements, not a benchmark or fresh-toolchain installation cost.

The first four unsuccessful drafts exposed proof engineering issues: reducing
generated tuple patterns before case splitting, normalizing the comparator and
suffix equations, and avoiding broad arithmetic preprocessing over a large proof
context. The final proof uses explicit next-index arithmetic and `omega`; its
local recursion-depth setting is the same 2048 used by the generated model. No
production refactor or custom axiom was introduced to make the proof pass.
Five new exported theorem axiom checks report only Lean's foundational
`propext`, `Classical.choice` and `Quot.sound`.

The final qualification used three independent Linux snapshots and shared only
the pinned dependency cache; project build outputs were separate. The compact
[lookup qualification receipt](evidence/lookup-validation-receipt.json) records
the exact frozen source, tool/command environment, exits, timings and log hashes.
Receipt SHA-256:
`5f5ab89db36dbe8194fd0e406243abe8fc2ecb25b711dff39fa049da6dac7a5b`.
Its source snapshot contains the four proof/code/mutation inputs, not subsequent
report edits. The new handwritten proof stayed byte-identical throughout:
`648e7e3f5a74401d1936bd3741e9562e86421ab2e6ad676a124fb02e2bbdb192`.

| Snapshot | Charon | Aeneas wall | Lean build | Direct lookup proof |
| --- | --- | --- | --- | --- |
| Baseline | 0.28 s, pass | 0.96 s, pass | 1.10 s, pass | 3.04 s, pass |
| Duplicate mutant | 0.14 s, pass | 0.72 s, pass | 3.79 s, generated model only, pass | 3.02 s, expected failure |
| Restored | 0.16 s, pass | 0.71 s, pass | 7.55 s, pass | 3.04 s, pass |

The baseline and restored standalone Rust suites both passed **2/2** (0.41 s and
0.13 s for the commands); formatting also passed. These are cached single-run
measurements, not end-to-end benchmarks or new toolchain bootstrap costs.

The [duplicate mutant](duplicate-mutant.patch) changes the second matching
entry's result to the previous Unique value. Rust compilation passes, fresh
extraction passes, and the generated Lean module compiles. The unchanged original
`SelectionProof.lean` also passes (2.54 s): its selection-only properties do not
specify lookup classification. The new proof fails at the duplicate-match
`pos.Unique` case with an unsolved False goal, rather than a parse/extraction
failure. Its failed-elaboration recovery placeholders are not accepted proofs.

The separate new Rust regression exits 101: two identical `(1, 42)` entries yield
`Unique(42)` under the mutant, while the test requires `Ambiguous`. The isolated
mutant kernel/model hashes are respectively
`8f3ed017976cd273acd50236244635596c8a5448413061ba63957b84304bfe1f` and
`b0e8f5a9a86bf927a85f81205503c9550a41ff658d7d0806512f218e4e1a3cf3`.
The restored run reproduces the baseline kernel/model hashes above and all five
new theorem axiom checks again report only the three foundational axioms.

Commands are the same manually invoked pipeline, with the pins in [README.md](README.md):

```sh
cd proofs/selection/rust
cargo test --locked --manifest-path Cargo.toml --lib
RUSTUP_TOOLCHAIN=nightly-2026-09-17 charon cargo --preset=aeneas
aeneas -backend lean -dest ../ hub_selection_proof.llbc
cd ..
lake build
lake env lean LookupProof.lean
```

The receipt also records setup mistakes separately: an initially incomplete PATH
could not locate rustup, an Aeneas invocation used the wrong LLBC working directory,
and an early mutant proof invocation preceded building its SelectionProof
dependency. Correcting the invocation/setup, without source or tool changes,
produced the recorded semantic experiment. Those setup failures are not evidence
that the semantic mutant was rejected. No full Hub/Docker tests, deployment or
model calls were rerun for this proof-only slice.

The source-level maintenance cost of this slice is the handwritten loop/specification
proof and its list lemmas, one manual Lake target, one Rust test and the negative
control patch. Actual future Rust or toolchain changes may require further proof
work; this run does not measure that future cost. Raw iteration and final logs
remain in `<workspace>/.cache/lean-pilot/lookup-evidence/`, outside Git.
