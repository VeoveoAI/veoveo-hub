# Guarded watcher composition: manual source boundary

The manual model and first foundational proof leaf are checked. The model compiled
in 8.232 seconds; the proof passed in 9.383 seconds with 18 executable axiom guards
(14 lemmas and four model definitions) and seven printed standard-only closures.
This is a current-projection/full-output foundation, not closure of the entire
watcher composition or a proof of async mutex/SQL execution. Runtime basis is
`a69e3cba3f9865e06e1db5ff77d1562ac468338a` (published coverage main `5653c8f`).
The existing Factory model and checked leaves remain unchanged. This layer calls
that same `apply`; it does not copy its statement executor or extract async Rust.

`FactoryWatcherModel` models one acquired per-runner critical region in one
`Watches::Inner`. Correspondence to Tokio guard ownership, no external row writes
and native query/transport effects is explicit; Lean does not prove scheduling or
lock acquisition. Query/preparation/log/status attempts remain in its trace when
stored state is unchanged. Logical preparation/resolve/fallback events represent
native steps, not a proof of every parser/tempdir/read/authorization operation.
Revision strings are opaque values and are never ordered.

| Model | Runtime responsibility |
| --- | --- |
| RunnerKey | Mutex/current_runner SQL key `(repository, branch, link)` |
| FailureKey/Failure | Watch status key includes watched deployment; full linked repository, parent revision, message and failed_at payload |
| project/RowAt/ReadAgrees/UniqueKey | RunnerRow conversion, indexed SELECT and actual instance uniqueness/nullability constraints; valid parsed identities/Row schema remain interpretation conditions |
| resume | `watch.rs::reapply` after candidate parent-ID parse and lock acquisition; rereads current projection before preparing |
| arguments/request | Current **applied** parent revision/pusher, current target, incoming or current fallback linked revision/pusher, source-returned own tree/entries; no correct Request is supplied |
| Source/factory | Native logical preparation response and unchanged Factory build/policy/provider/SQL/RPC observations, indexed by actual requested source arguments |
| preparedName | Actual tracked-ID parse observation and any successful branch name result, including one marked deleted; errors preserve recorded current name |
| attempt | Caller-locked change_runner invokes Factory.apply, logs/swallowing its error; Factory failure is not watcher preparation failure |
| publish | runner_failed upsert/runner_ran delete of the exact status key; ack and commit are distinct observations |
| parentLink | Existing-runner portion of guarded run_links query/resolve/recorded/apply/status; query Err logs and supplies existing=None, not a no-op |

The current query returns a projection, not all 21 Row fields. `ReadAgrees`
expresses the native SELECT/parse observation's relation to store rows; the full
backing Row remains separately bound to the actual Factory lookup. Its unchanged
initial binding is required whenever Factory.apply is invoked. `parentLink`
requires that existing-row binding explicitly. Native new allocation after a
successful missing-row lookup is outside this inherited existing-runner domain;
it is not modeled as a native skip or a fabricated allocation result.

ParentInput retains parent BranchId and link follows/pinned inputs, request target,
current source membership/program arguments and context. The resolve observation
receives these plus the queried optional current projection. Its implementation
(Lore watcher/grants, branch resolution, committer/pusher selection, policy/read
checks) is an external source contract. On resolve error, recorded fallback is
called using the parent's REQUESTED linked Watched but may RETURN the existing
link's different repository/target/revision/pusher. No requested-target equality
is assumed on that fallback. The resolve failure is still upserted even if its
recorded fallback's Factory operation succeeds.

Cancellation has separate after-lock, before-change and before-publication
observations. Native correspondence requires their monotonic consistency. A
status write acknowledged as successful is committed; error/lost reply may have
committed or not. The model retains those choices and complete payload, not an
unconditional successful-clear premise. Native UTC time maps to Timestamp; no
global clock freshness/injectivity assumption is added.

Explicitly excluded: saved parent retry/membership freshness, cross-replica/direct
writer interleavings, registration/served_at writes, general row allocation or
complete native removal/database cleanup, parser/crypto/provider correctness,
crash/nonreturn, automatic retry and liveness. A completed row-deletion boundary
can support no-resurrection composition, without claiming to prove native cleanup.
Unrelated native-object frames retain the earlier scoped no-alias condition for
(workspace, truncated sandbox name); SQL row/key framing uses full keys.

The checked proof derives independent no-op/accepted outputs, SQL projection
correspondence, whole-operation capture independence, constructed current-parent
arguments and complete Factory outputs. Finite two-operation parent retarget and
same-target advancement, status/fallback/error ordering, unrelated-key failure
frames and populated client-facing witnesses remain next. The two later controls change only post-relevance metadata selection (same-target
program advancement) or add an overstrict captured-parent equality refusal.
They will not mutate the actual backing Row/current SQL observation or reuse stale
proof artifacts under a changed model.


## Model elaboration receipt

The first direct model attempt failed in 21.754 seconds on four ambiguous trace
constructor identifiers; it produced no accepted artifact. The retry qualified
those constructors without changing semantics. The model source is
`8cef39ee9a74201321f26a2ccc5c677d4794aec20d94a8e4dbf2fb26c05ed345`.
The direct command was `LAKE_NO_UPDATE=1 lake env lean FactoryWatcherModel.lean
-o .lake/build/lib/lean/FactoryWatcherModel.olean`, with a 180-second bound.
The successful log was empty (SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`).
All source/import maps remained unchanged. The exact accepted Factory model
artifact `86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee`
was reused after source/dependency gates. GPT-6 Luna ran the bounded Linux check;
GPT-6 Astra reviewed the result. No native tests were repeated for these proof-only
edits. The separately checked proof leaf is recorded below.


## Checked foundation receipt

`relevant_full_output` constructs legitimate acceptance from no-fault/capacity
observations and the current SQL projection, then reuses the checked Factory
complete-output proof. It never assumes a correct Request or desired after-state.
`capture_independent` preserves the whole returned operation when only stale
captured metadata changes, with key and incoming tracked ID fixed. Obsolete or
absent operations preserve the whole stored Factory/failure state while retaining
the actual modeled query/log attempts. None of these statements verifies native
mutex acquisition, complete resolver behavior or automatic recovery/liveness.

```json
{
  "status": "checked manual watcher foundation; remaining composition/witness/control obligations open",
  "runtime_source": "a69e3cba3f9865e06e1db5ff77d1562ac468338a",
  "inherited_apply_source": "48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c; apply_to body unchanged",
  "lean": "4.35.0",
  "mathlib": "56a005855ea17e0b0958f73ec801ae140b0a1679",
  "aeneas_used": false,
  "source_hashes": {
    "FactoryWatcherModel.lean": "8cef39ee9a74201321f26a2ccc5c677d4794aec20d94a8e4dbf2fb26c05ed345",
    "FactoryWatcherProof.lean": "1f78591915464bd2b37f34d3887e351809e1785225d77f4fa121781629ba04a3",
    "lakefile.lean": "afa8da141bc2b819144fb8bc85e91577e15040bff44bb9955446ae741bd48b06",
    "lake-manifest.json": "e961af39664462e4132c9dbe81503dcdec33810fec18a1b0c3aae6765c8d9286",
    "lean-toolchain": "ca5f1f601d34c49079e38d7e7041eb0f2a9c227fbc1df0f404820069a052330c"
  },
  "model": {
    "elapsed_ms": 8232,
    "exit": 0,
    "input_sha256": "a11afee06c1d862f4938c865871dcc6ef6bc5f7c9bd75e6908a488db345b0be6",
    "source_map_sha256": "c1fab59367a918f61ac65c586482ddd31f49193ad1c44e96c6e40716cb23df80",
    "olean_sha256": "f0dc55f6804747bf83d40aadf572cfeaee9413162ee0907d5f7c8cc51aa5cbef",
    "olean_bytes": 2450168,
    "log_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "log_bytes": 0
  },
  "foundation_proof": {
    "elapsed_ms": 9383,
    "exit": 0,
    "input_sha256": "ae53df121e0ca84be3bead86850dc6b826ad2669a03ac60d301b2abc5c6100ec",
    "source_map_sha256": "eeb08170066339be4273e8b52ec8b5f8d3b0f9026704e2f69c3328b91bde3c42",
    "source_count": 11,
    "olean_sha256": "fdf6f9d89f732ee0206f8a296a92950a9c4357be90be9bcc83ad047b49f750bd",
    "olean_bytes": 711296,
    "log_sha256": "5b47070f92396614e2891c35c3e08d60bb206ae5a0a223e6919eab001d98921c",
    "log_bytes": 1077,
    "axiom_guards": 18,
    "guarded_lemmas": 14,
    "guarded_definitions": 4,
    "printed_closures": 7,
    "allowed_axioms": [
      "propext",
      "Classical.choice",
      "Quot.sound"
    ],
    "sorryAx": false,
    "nonblocking_warning": "publish_exact includes an unused DecidableEq P section instance; accepted source retained exactly"
  },
  "prerequisites": {
    "FactoryRetargetModel.lean": {
      "source_sha256": "52f6da22b5acc42fdff56c6f0d32c28996f1953ce3da958a23c8fe9fbce51bf0",
      "olean_sha256": "86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee"
    },
    "FactoryRetargetProof.lean": {
      "source_sha256": "ea62b2e5ef873b87fd1c5eb0b31668fb365ed2f339f5057527194c3d8988fead",
      "olean_sha256": "7a72412270c42a64c968935a531750770bb3456f7eaa92c7deb1613711aa7564"
    },
    "FactoryNativeOutputProof.lean": {
      "source_sha256": "0165725f865ae28f1d4e7e3530bec9c0d7fa088bf4a3246c83e9335c4b3bba23",
      "olean_sha256": "e2c9136699ef0cc9ea71096bce41a458f0068bd9c2cc17fa42b38df0bbef04de"
    },
    "FactoryRecoveryProof.lean": {
      "source_sha256": "80f34ec4401428dbabf13e8dac061e2546974377ffc08dc0b92f695c1ebabc35",
      "olean_sha256": "30c0ee7c55eebba0d377adf80d6d3c0169d13ac22479ea553b2208424aa01e43"
    },
    "FactoryChoiceProof.lean": {
      "source_sha256": "81198160608ac4d811d679e3ee4e94facd4ffd5c9be3a4e1ce70f4077bb3181a",
      "olean_sha256": "682c649e911d0e42225397aa882ace22d7905ff5c82686c3496b276d8e0b663f"
    },
    "FactoryWatcherModel.lean": {
      "source_sha256": "8cef39ee9a74201321f26a2ccc5c677d4794aec20d94a8e4dbf2fb26c05ed345",
      "olean_sha256": "f0dc55f6804747bf83d40aadf572cfeaee9413162ee0907d5f7c8cc51aa5cbef"
    }
  },
  "dependencies": {
    "lean_binary_sha256": "bf8d54e4714cc4b03d3f6bb34c83b7202b87e49c8bfcbff6895c085bb90ceb38",
    "lake_binary_sha256": "8ba83c98f76562d03b8dbc33f1057151ba6cbb121679dde76b2260ccc90495e1",
    "Mathlib.Tactic.olean": "589150e06f904bce5b615763147d60460598bee3ed3c5307ea6f7217d1944ce2",
    "mathlib_manifest_sha256": "6fcb8417bf0d6056cebf3c9f9bacd4569f21f3b9c571c60649150b9118b55147"
  },
  "sources_and_imports_unchanged_before_after": true,
  "authoring_diagnostics": [
    {
      "case": "watcher-model-check1",
      "elapsed_ms": 21754,
      "exit": 1,
      "log_bytes": 1448,
      "log_sha256": "e6c52601a9a3f100c38676c58e8f2264435f2ace9dd8ddd3297513f9e46bb695",
      "cause": "four constructor inference errors in parent fallback trace; qualification only, no semantics changed",
      "accepted_olean": false
    },
    {
      "case": "watcher-foundation-check1",
      "elapsed_ms": 12487,
      "exit": 1,
      "log_bytes": 6558,
      "log_sha256": "39fb7ea80dffa744b247813d124a22bae75a5d283f55b5daae19a381c06c3b5b",
      "cause": "function extensionality/statusAfter reduction and full projected-record substitution; no false theorem counterexample; audit rejected unfinished dependencies",
      "accepted_olean": false
    }
  ],
  "publication_note": "Only package prose/receipts changed after the checked snapshot. Model/proof/config bytes remain exact; no .olean, log binary, native source or new runner committed."
}
```

Manual commands (180-second bounds in the recorded private runs):

```sh
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherModel.lean -o .lake/build/lib/lean/FactoryWatcherModel.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherProof.lean -o .lake/build/lib/lean/FactoryWatcherProof.olean
```

The first check elaborated the new model against the exact inherited model
artifact. The proof check reused six source/hash-gated prerequisites; it did not
rebuild earlier proofs or rerun native tests. GPT-6.1 Sol authored the package,
GPT-6 Luna ran bounded Linux validation and GPT-6 Astra reviewed complete logs,
contracts and correspondence. Guards are dependency audits, not execution cases.
The larger watcher slice remains partial until parentLink/queued-candidate and
status compositions, constructive witnesses and selected controls/restorations are
qualified. This foundation does not narrow those remaining requirements.
