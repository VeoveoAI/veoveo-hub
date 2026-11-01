# Guarded watcher composition: manual source boundary

The manual model, foundation, parent/queued-update histories and status/recovery
proof leaves are checked. The model compiled
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

The foundation derives independent no-op/accepted outputs, SQL projection
correspondence, whole-operation capture independence, constructed current-parent
arguments and complete Factory outputs. The checked history extension below now
composes actual parent retarget and same-target advancement with queued updates,
deriving intermediate key/row uniqueness from the actual first operation. The
status extension now checks failure/fallback ordering and a bounded deletion
history. Populated client-facing witnesses and semantic controls remain next.
The two later controls change only post-relevance metadata selection (same-target
program advancement) or add an overstrict captured-parent equality refusal.
They will not mutate the actual backing Row/current SQL observation or reuse stale
proof artifacts under a changed model.

## Checked parent and queued-update histories

`FactoryWatcherHistoryProof` constructs the parent's request from its source input
and native resolution observation. `parent_full_output` derives accepted execution,
the full row/native/provider/create-request output and complete client projection
from no-fault/capacity facts. The next operation receives that actual resulting
state. Row identity, key membership and `UniqueKey` preservation follow from the
inherited `apply_identity` theorem; the desired intermediate state is not assumed.

`parent_retarget_then_obsolete` proves whole-state preservation after a repository
change **or** a tracked-branch-ID change in the same repository. It retains the
first operation's complete client projection and allows a query error in the
second operation. `parent_program_then_relevant` accepts an old captured candidate
when its target still matches, uses the current applied parent's revision/pusher
and source response, and preserves full outputs, client identity, failure-map
effects and unrelated Factory state across both calls. No equality with captured
parent metadata is required. Returned source contents and native observations are
still external contracts; issued `branch@LATEST` argv does not establish checkout
bytes at the recorded revision.

The leaf also proves that parent query failure still calls resolve with `None`,
and that status publication preserves every other complete failure key. At the
history checkpoint, recorded fallback, preparation-refusal ordering, deletion
histories, populated witnesses and controls remained open; the status checkpoint
below now covers the first three.
All earlier native/SQL/process-local interpretation limits above still apply.

GPT-6.1 Sol authored the leaf, GPT-6 Astra reviewed the statements and complete
compiler output, and GPT-6 Luna ran the manual Linux check. Lean 4.35.0 and the
existing Mathlib pin are unchanged. The command, bounded at 180 seconds, was:

```sh
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherHistoryProof.lean -o .lake/build/lib/lean/FactoryWatcherHistoryProof.olean
```

The successful check took **5.773 seconds**, with 23 executable axiom guards
(19 lemmas and four definitions) and seven printed closures admitting only
`propext`, `Classical.choice` and `Quot.sound`. Unused section-variable and simp
argument warnings remain; no additional axiom or unfinished proof was accepted.
The 12-file frozen source map and seven accepted prerequisite source/artifact
pairs were unchanged. The new output was absent before compilation. The direct
target remains manual and nondefault; runtime, earlier models and proofs are
unchanged, and no native tests or deployment were repeated.

The first attempt failed in 19.701 seconds with no output artifact: a reserved
identifier, a case-rewritten row binding and a record-equality tactic failed.
Missing helper and `sorryAx` diagnostics followed those errors. Only those three
proof-authoring sites changed for the successful check; explicit conditions and
conclusions did not change. These were not platform defects or semantic controls.

```json
{
  "scope": "manual parent/queued-update histories; whole watcher slice still partial",
  "runtime_source": "a69e3cba3f9865e06e1db5ff77d1562ac468338a",
  "proof_main_basis": "2ea8eb8",
  "source_sha256": "4071052dd897c4fab68c1d81dd3b12e952eef2076ec1029edebd05d91d2efb1c",
  "source_bytes": 26424,
  "input_sha256": "3ad782244150f39e6d9bddeebf8609c4d4a00b5fdc9c1a08d8820c83b0cf6e0d",
  "archive_sha256": "bc363747adb75a91a63020eb32405d04c4db90ce23e577032a7d69b8c37d2201",
  "source_map_sha256": "58e2140081e30aa1b2626d2a27843206941a90dc8ff90f536a896c64a0225bcd",
  "elapsed_ms": 5773,
  "exit": 0,
  "guards": 23,
  "printed_closures": 7,
  "artifact_bytes": 710016,
  "artifact_sha256": "05b0abe7dcd9e4c991f90f6fb234932c0a7e2eaca01145e6a5378cf5b025da9c",
  "log_bytes": 4482,
  "log_sha256": "5b5bee4a3ad6984c714bcb7381d40d8922b5b5db7e9e7c388e087aecbe28bfd2",
  "summary_sha256": "a66b9d8f093d050bc159debc579bd414b60e3b04df2346c126e6599161cbb4a1",
  "failed_attempt_log_sha256": "bc9088b334f84bb504c5b356a03cbca971285db77aebb0027630a01f9dc4c16c"
}
```

Temporary evidence is under
`/tmp/hub-factory-concurrent-preparation-20261031/watcher-history-check2-mirror/evidence-run1/`;
the first attempt is in the corresponding `watcher-history-check1-mirror` directory.
The [dated history record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-histories.md)
keeps this checkpoint separate from native and deployed evidence.


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
The parent/queued-candidate and status/deletion compositions are now checked in
the extensions. The larger watcher slice remains partial until constructive
witnesses and selected controls/restorations are qualified; the intermediate
checkpoints do not narrow those requirements.

## Checked status, fallback and deletion boundaries

`FactoryWatcherStatusProof` calls the accepted model and history contracts without
changing them. It adds nine lemmas and the `rowDeleted` boundary definition:

- `refusal_then_parent_history` derives a full preparation failure and preserved
  Factory row from an actual first `resume`, then runs an actual `parentLink` on
  that resulting state. Complete Factory/client outputs and both keyed status
  effects are retained. Only a committed second publication clears the earlier
  complete failure, and every other failure key is framed. A different watched
  deployment is proved to be a different key even for the same runner.
- `successful_fallback_records_resolve_failure` constructs accepted fallback
  application from no-fault/capacity observations while retaining the original
  resolution failure. Status names the requested linked repository; client and
  applied row name the returned fallback target, which may differ.
- `factory_failure_is_not_preparation_failure` characterizes an observed Factory
  failure with the inherited exact failed-row prefix, while preparation succeeded
  and watcher-status clearing was attempted. It does not assume that Factory
  failure is preparation failure or that a clear attempt committed.
- `completed_delete_then_obsolete` derives key absence after the explicit row
  deletion, using prior uniqueness, then proves whole-state preservation by a
  queued notification with a consistent reread. `rowDeleted` changes only the row
  map; this is not a model of complete native sandbox/provider/database cleanup.

SQL-read/native correspondence, one acquired process-local critical region,
normal return and the previous schema and external-observation conditions remain
explicit. The status contract retains the whole failure value (repository,
revision, message, timestamp) and distinguishes commit from acknowledgement.
Generic failure-prefix characterization supplies the observed Factory failure as
a premise; constructive populated failure witnesses remain next.

The first direct check passed in **5.473 seconds**, with ten executable axiom
guards and six printed closures allowing only the three standard axioms. One
unused-section-variable warning remains. All 13 frozen files and eight accepted
prerequisite source/artifact pairs were unchanged; the new output was absent at
the start. GPT-6.1 Sol authored, GPT-6 Astra reviewed statements and full logs,
and GPT-6 Luna ran the same pinned Linux toolchain with a 180-second bound:

```sh
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherStatusProof.lean -o .lake/build/lib/lean/FactoryWatcherStatusProof.olean
```

```json
{
  "scope": "manual status/fallback and completed row-deletion composition",
  "runtime_source": "a69e3cba3f9865e06e1db5ff77d1562ac468338a",
  "proof_main_basis": "dd6bd84",
  "source_sha256": "b44a4e8a4defb755c3b2f9cbe1a85929e54596b030385ef46f40cc28d3dc694c",
  "source_bytes": 16248,
  "input_sha256": "7cc17bdea5fa59284e4ca649e23a5f44e0f138e38ec0ef661fa6288e6419157c",
  "archive_sha256": "a75691311ad2759093672a2de93c6a88718f21b7eeb52b283c87e9beace7e9b9",
  "source_map_sha256": "e0d3e1c5ec4c801a4dac232f22aa9f879ca643f9b33cd2fc4bfa083b351ef383",
  "elapsed_ms": 5473,
  "exit": 0,
  "guards": 10,
  "printed_closures": 6,
  "artifact_bytes": 520880,
  "artifact_sha256": "7aef310c436e570c391ca00e8873c41f80b66a4d1210d5a70182248725390865",
  "log_bytes": 1135,
  "log_sha256": "0f01580784fdaafff9091f15cf512010e946e48100ee4e9b0198ac2dee4ade17",
  "summary_sha256": "db761f9a0568e3694c5da8e5afd712b4819d40eb128e246501d52774bd2bbbaa"
}
```

Evidence mirror:
`/tmp/hub-factory-concurrent-preparation-20261031/watcher-status-check1-mirror/evidence-run1/`.
The [dated status record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-status.md)
keeps this proof-only result separate from native tests and deployment. No runtime,
dependency, accepted model or earlier proof changed, and no native check or paid
model call ran. Populated complete outputs and both semantic controls/restorations
remain necessary before closing the watcher slice.
