# Guarded watcher composition: manual source boundary

The manual model, foundation, parent/queued-update histories, status/recovery
proofs, populated examples and both selected semantic controls/restorations are
checked within the stated manual-model scope. The model compiled
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
history. Six populated client-facing histories and the two selected controls
are checked below. The controls change only post-relevance metadata selection
(same-target program advancement) or add an overstrict captured-parent equality
refusal. They preserve the actual backing Row/current SQL observation and use
fresh derived artifacts for each model change and exact restoration.

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
the extensions. At this foundation checkpoint the selected controls/restorations
were still required; their later qualification is recorded below. The native and
concurrency boundaries remain unchanged.

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
were subsequent obligations at this status checkpoint; both are now recorded
below within the same explicit native interpretation boundary.

## Checked populated watcher histories

`FactoryWatcherFixture` imports only `FactoryWatcherModel`, which imports only
`FactoryRetargetModel`. Its separate proof leaf imports no generic correctness
proof. Six closed execution assertions construct actual operations and compare
independent full rows, client identities, native/provider observations, request
lists and argv, plus complete status values at the affected and unrelated keys:

1. A parent retarget from repository A to B, followed by the captured A event.
2. A retarget from A/branchA to A/branchB, followed by the old branchA event.
3. Parent2/Frank replaces parent1/Alice while still targeting A; the captured
   candidate accepts the incoming linked update using current own entries and
   image observations, with both issued requests and the complete client visible.
4. A selected native delete error constructs the exact failed row/event prefix
   while logical preparation succeeds and its watcher failure is cleared.
5. Preparation refusal commits the full timestamped failure despite a lost reply;
   an actual later parent operation clears it and preserves another deployment's
   status for the same runner.
6. Resolution of requested B fails, recorded A applies successfully, and the
   timestamped B error remains even though its publication reply is lost.

The initial projection, runner shape, singleton-key uniqueness and source-oracle
indexing also pass. The oracle indexes BOTH parent revision and pusher, with
different old/current tree, entries, image and native observations; crossed
coordinates refuse. The second operation's backing row and SQL projection come
from the first actual result, not an expected intermediate row premise.

Before freezing, review corrected an inconsistent following-link input to pinned
links with coherent pinned revisions/returned branch IDs. This was a fixture
correction, not a production change. Schema, parsed-identity, digest, URL/protobuf
and native-observation correspondence remain explicit. Separate create/provisioning
and later ready observations do not prove a native readiness transition. The
696-byte runner script exactly matches current Rust formatting (SHA-256
`dd5d553375a987f734834cacf6dd72c8eddb7f15c9b41c29bf05f284b952b9d1`);
all four public synthetic raw-token/digest pairs were independently checked.
Neither this data audit nor the issued branch@LATEST argv establishes checkout
bytes or native execution.

GPT-6.1 Sol authored, GPT-6 Astra reviewed source/data and full logs, and GPT-6 Luna
ran the pinned Linux manual check. Fresh fixture compilation took **5.474 s**;
the six-execution proof took **92.718 s** (98.193 s combined), each bounded at
180 seconds. Ten guards and six printed closures pass with only the standard
three axioms. Local `maxRecDepth 4096` applies to the six execution assertions;
no global limit or runtime behavior changed. The seven-file source map and two
accepted model import pairs stayed unchanged. No native/application tests,
deployment or paid calls ran.

```sh
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherFixture.lean -o .lake/build/lib/lean/FactoryWatcherFixture.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherFixtureProof.lean -o .lake/build/lib/lean/FactoryWatcherFixtureProof.olean
```

```json
{
  "scope": "six synthetic populated watcher histories; semantic controls pending",
  "runtime_source": "a69e3cba3f9865e06e1db5ff77d1562ac468338a",
  "proof_main_basis": "abfb89b",
  "fixture_source_sha256": "95d51027444224916177170887fc645466f5cfaf72cefc647b88f4258d7077ca",
  "fixture_source_bytes": 24962,
  "proof_source_sha256": "e66abae0f8ba792c3dee725648fc2fa69ecd17fc8a81878cc2750be488bd3a6f",
  "proof_source_bytes": 4925,
  "input_sha256": "b75d802924c4b67cee11ddfb94c723c8cdcc8f16b96e3b2bd2878289627c63fc",
  "archive_sha256": "4c5dda56936fde62c8a97014af7428c0044a215d64f06e75ea82935a3731fef4",
  "source_map_sha256": "38792a044940d74ff78a5dc0879d91ade28ca7ceb01730c080e2a6d5a82bae7c",
  "fixture_elapsed_ms": 5474,
  "proof_elapsed_ms": 92718,
  "combined_elapsed_ms": 98193,
  "guards": 10,
  "printed_closures": 6,
  "fixture_artifact_bytes": 929656,
  "fixture_artifact_sha256": "a95c2fd70cad74f7dc156ef8786796691259b8fef0479420be6b43236070e553",
  "proof_artifact_bytes": 240368,
  "proof_artifact_sha256": "c3c7f44334e16a525c61aa92214ecb0d3c5047635f27c70b6f959f7addf3bab5",
  "proof_log_bytes": 778,
  "proof_log_sha256": "0e5dfbe1beadb535f0b1985eb784ec7b84c9f1778a50ae207d9c4127fdd1f820",
  "summary_sha256": "27ace507de8ba6859b7adf863a43174ad621dd9d792d293b9647acfcc476d97c"
}
```

Temporary mirror:
`/tmp/hub-factory-concurrent-preparation-20261031/watcher-fixture-check1-mirror/evidence-run1/`.
The fixture log is empty. The [dated record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-fixtures.md)
retains this qualified baseline. Its receipt above preserves the then-pending
control status; the next section supersedes that historical status without
changing the baseline source or proof bytes.

## Checked watcher semantic controls

Both deliberate handwritten-model controls and exact restorations are qualified;
this supersedes the historical pending-control statuses above. GPT-6.1 Sol authored,
GPT-6 Luna ran the pinned Linux checks, and GPT-6 Astra reviewed every full witness,
rejection and restoration log/map. The positive declaration is
`FactoryRetarget.Watcher.Fixture.current_parent_program_accepts` in module
`FactoryWatcherAdvanceControl`. It imports only Fixture/Models, never generic
History/Status proofs. No runtime/model/prior-proof
bytes changed; packaging adds only its nondefault target and exact control files.

- **Lost metadata:** selecting the candidate after the intact same-target relevance
  check leaves the real backing/current SQL data intact, but writes old1/Alice's
  program. The independent witness checks the complete wrong row, client,
  providers, native observation, request/argv and actual preparation arguments.
- **Wrong refusal:** the extra captured-parent equality guard skips an event with
  usable current target/source. Its independent witness checks the full refusal,
  exact skipped trace and whole modeled state preservation.

Fresh Model/Fixture/witness stages passed before each unchanged positive assertion
reported `decide` false. Its cascading `sorryAx` was rejected and no positive
artifact was emitted. No parser/import/type/resource failure counted as detection.
The rejection logs are identical, but the independently proved wrong outcomes
differ. Accepted witness closures use only the three standard axioms (one guard
for lost metadata, two for refusal). These are manual-model faults, not native
Rust extraction/refinement or new runtime bugs; the prior native race/fix is
separate evidence.

Each exact reverse patch restored all ten private sources. The whole local build
graph was removed except the hash-gated immutable RetargetModel; WatcherModel,
Fixture and positive assertion were rebuilt. Restored baseline artifact hashes
match. The final restoration also rebuilt all six FixtureProof scenarios: the
combined positive chain has **11 guards/seven prints**, comprising AdvanceControl
one/one and FixtureProof ten/six. These are dependency audits, not eleven scenarios.
The finite local recursion bound remains 4096. Lean/Mathlib/tool/shared-model
identities and native interpretation boundaries are unchanged from the earlier
receipts; no native test, deployment or paid call ran.

### Isolated reproduction

Use a scratch copy and require a fresh positive baseline first. Only the exact
RetargetModel artifact may be reused; after every model change, remove all local
derived artifacts for WatcherModel, Fixture, witnesses and controls. Recorded
commands had 180-second process-group bounds (patch application: 30 seconds).

```sh
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherModel.lean -o .lake/build/lib/lean/FactoryWatcherModel.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherFixture.lean -o .lake/build/lib/lean/FactoryWatcherFixture.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherAdvanceControl.lean -o .lake/build/lib/lean/FactoryWatcherAdvanceControl.olean
```

Copy `mutations/FactoryWatcherLostMetadataWitness.lean` to the scratch root, apply
`patch --batch --fuzz=0 -p1 < mutations/lost-current-metadata.patch`, clear derived
artifacts and rebuild Model/Fixture. Then run:

```sh
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherLostMetadataWitness.lean -o .lake/build/lib/lean/FactoryWatcherLostMetadataWitness.olean
LAKE_NO_UPDATE=1 lake env lean FactoryWatcherAdvanceControl.lean -o .lake/build/lib/lean/FactoryWatcherAdvanceControl.olean
```

Require witness success then specifically `decide`-false rejection/audit, not a
setup failure. Reverse the exact patch using `patch --batch --fuzz=0 -R -p1`, verify
the complete baseline source map, clear derived artifacts and rerun the positive
commands. Repeat independently using `refuse-captured-parent-change.patch` and
copied `FactoryWatcherFalseRefusalWitness.lean`. Both files live in `mutations/`
and have no baseline targets. After the final restoration, also run the full
`FactoryWatcherFixtureProof.lean` command given above. No generic proof artifact
crosses a model mutation.

### Compact receipt

Durations are compilation/check times, not native operation latency. Artifact/log
aliases below mean `[bytes, sha256]`; `none` means no artifact. Each mutant's last
exit 1 is the reviewed intended rejection, following successful witness stages.

| Run | Stage | Exit | Seconds | Artifact | Log |
| --- | --- | --- | --- | --- | --- |
| baseline | Model | 0 | 10.547 | model | empty |
| baseline | Fixture | 0 | 4.677 | fixture | empty |
| baseline | AdvanceControl | 0 | 20.666 | positive | positive |
| lost | Model | 0 | 8.429 | lost-model | empty |
| lost | Fixture | 0 | 4.822 | lost-fixture | empty |
| lost | LostMetadataWitness | 0 | 22.912 | lost-witness | lost-witness |
| lost | AdvanceControl | 1 | 9.933 | none | rejection |
| lost-restored | Model | 0 | 5.523 | model | empty |
| lost-restored | Fixture | 0 | 3.770 | fixture | empty |
| lost-restored | AdvanceControl | 0 | 22.504 | positive | positive |
| refusal | Model | 0 | 6.976 | refusal-model | empty |
| refusal | Fixture | 0 | 3.771 | refusal-fixture | empty |
| refusal | FalseRefusalWitness | 0 | 36.687 | refusal-witness | refusal-witness |
| refusal | AdvanceControl | 1 | 10.840 | none | rejection |
| final-restored | Model | 0 | 6.778 | model | empty |
| final-restored | Fixture | 0 | 4.573 | fixture | empty |
| final-restored | AdvanceControl | 0 | 25.919 | positive | positive |
| final-restored | FixtureProof | 0 | 92.624 | fixture-proof | fixture-proof |

```json
{
  "runtime_source": "a69e3cba3f9865e06e1db5ff77d1562ac468338a",
  "published_fixture_basis": "980bef64e9550ef68ef27e8272a572b7e46a50d3",
  "shared_private_archive_sha256": "fd68f99c910dd5dd3311373140798e4711bd37683bda4739a0478eb5c88ac7ef",
  "source_maps_sha256": {
    "baseline_and_both_restorations": "31667665f3e29b9003f189bfcbc9d659763f3bb1926f2fd7e2a006d2c4f3515c",
    "lost": "4894951e002cf396c5e650d3a482c8506fc894e761cf568c13745b12279a22e8",
    "refusal": "8254b5285c3e3f5b94fdbf739d0d8aad5301d9a24e211ab2e345be01e9b29d25"
  },
  "mutant_model_source_sha256": {
    "lost": "be5f598beb1bcfc3faa2514325768665ae072736b5512e7b3f40a74157d92937",
    "refusal": "3d0aee00e0e6d6ce0f3719880b349f2392bc6e02650298490cfe6e23f982b654"
  },
  "shared_patch_log_sha256": "4d35d1f3c43531897ea4c6b03373be65fa49078a59a14be0fe9338a76c95d325"
}
```

New source/patch identities; unchanged model/fixture sources retain the hashes in the earlier receipts:

```json
{
  "FactoryWatcherAdvanceControl.lean": "9c91c685286a065e40c8577825215a5f66cc30296fc4b2d35a129b12f887b514",
  "mutations/FactoryWatcherLostMetadataWitness.lean": "fc2ffc054df8e85f706d16455f60dd887f0ebc479feadf07e53596597681805a",
  "mutations/FactoryWatcherFalseRefusalWitness.lean": "65450bff84bcdee037a3f1be67226744d8d95686fec9ead8c829bb8148f7eb8c",
  "mutations/lost-current-metadata.patch": "7ac7eb4af011cf8392c93b70b4f55a5f8615b45bbee3ba7fff60444a17deb455",
  "mutations/refuse-captured-parent-change.patch": "fad27543fc265772883b5787512dc6adfae15e026f765b4da6344a60075c900d"
}
```

Deduplicated artifact and log identities:

```json
{
  "artifacts": {
    "model": [2450168, "f0dc55f6804747bf83d40aadf572cfeaee9413162ee0907d5f7c8cc51aa5cbef"],
    "fixture": [929656, "a95c2fd70cad74f7dc156ef8786796691259b8fef0479420be6b43236070e553"],
    "positive": [35112, "d14b1106f7d84738cc49c700f4fdd30830dcebc2c6975ccd0ac01d1fae937e3d"],
    "lost-model": [2450136, "368d6862408966c6d622b17480db6b508f0d0cff584e3d6ec2caa21dc8599215"],
    "lost-fixture": [929712, "481170817ace62c8fcddd0f5c96367caa7824e1177a4c9745b567edba56858a5"],
    "lost-witness": [75824, "68342b3858b49e62ae71b533721d99f990612239fdafbc406c9e7bce508c2bb4"],
    "refusal-model": [2456328, "9e1fe53c65cb6c92beb82276e4fafbd5226897759e1c2b8a4e6d234a59a5d7c7"],
    "refusal-fixture": [935408, "0cfe72ec7c83ce2376600b7c559e8620fa4bffb13e46c556509b85603d4dafcd"],
    "refusal-witness": [62416, "e27ae3d17f55bc5f7638803f0b43ada3f6cfc9f05d182e0be71aee9fec575e35"],
    "fixture-proof": [240368, "c3c7f44334e16a525c61aa92214ecb0d3c5047635f27c70b6f959f7addf3bab5"]
  },
  "logs": {
    "empty": [0, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
    "positive": [126, "6fa4998cf1d8a309527052480c628017ae2e224cef05fdac1f34e66da8899eca"],
    "lost-witness": [130, "5c58750c0cbab327d96329eaef7501ba9e8f8b0ef8c22c63a52c08bf3744eea6"],
    "rejection": [2190, "6b0b6cd5015c423d694ec197b7d21691483cd5e07796551dda9f46a3bb7f4d73"],
    "refusal-witness": [265, "04d540703f0572c60b16670b3b195c2ca3a9ffdcff55d860cdd94e4be718f221"],
    "fixture-proof": [778, "0e5dfbe1beadb535f0b1985eb784ec7b84c9f1778a50ae207d9c4127fdd1f820"]
  }
}
```

Run totals and summary/input/runner identities:

```json
{
  "baseline": {"elapsed_ms": 35894, "last_stage_exit": 0, "summary_sha256": "1917302343a0b0924310fcb7dac209917e19a451466df0cf50beb0e8e7920922", "inputs_sha256": "2098e91288a0154346ff7254e7a683d474ae8e9e8b8a088093d542fd8eef62ff", "runner_sha256": "99596b4f9958e6262e3599b37798ef12076927fae2a4f3cd1b8b22faa18c7a41"},
  "lost": {"elapsed_ms": 46100, "last_stage_exit": 1, "summary_sha256": "a7cd6a12a8c84b90d1bbaeff84c8d2ce5d3300159ca474fd1f9632591a3527a7", "inputs_sha256": "17c83968506e5d785996eb690faf8bbf68e85933e72e3d04b0fd31b7771c009e", "runner_sha256": "985db04e9732d42189adfddc66871251bd2f79ff3266fee8c2b6ea20244c45d9"},
  "lost-restored": {"elapsed_ms": 31801, "last_stage_exit": 0, "summary_sha256": "6b1ec504b08fbe35f4e90888fc1bffdff8ab52faf4154301a0eb7c491c8588d6", "inputs_sha256": "220a99d9a439ab66688c8c7d94621a0cbfbc24466e116e21f3c0e82e19c164f2", "runner_sha256": "ea4f5aaffdfcf2945b1e58a8048e922043011013614e3d0e37632f48f9fcd4e6"},
  "refusal": {"elapsed_ms": 58278, "last_stage_exit": 1, "summary_sha256": "8fc54c872ea29163e6897a65087c5eddbaeb3791497054dfa4e5e67e1c5a02fd", "inputs_sha256": "8fbdead07ad98aa5f4b702097242d7149cfb69bb457b2cf86d83c66b52a10f0a", "runner_sha256": "4cc75294cd3aa877cc633443be6a4cddea0bc3d272db73e70f1606b0a9417fdb"},
  "final-restored": {"elapsed_ms": 129899, "last_stage_exit": 0, "summary_sha256": "06cfa18115840306fae4296177a4897bd411bcc702bc6b843880af3e2639d5ef", "inputs_sha256": "4fb84c50258b70fbdc7b97c5008c41cc362857fe19acfbc84d10320335167d2d", "runner_sha256": "f7b822083ffa2361759d0e544fa4ec8233db8eaff925ba31572a669fa3b865e3"}
}
```

Mirrors: `/tmp/hub-factory-concurrent-preparation-20261031/watcher-controls-baseline1-mirror/evidence-run1/`
and, under the same base, `watcher-controls1/{lost-current-metadata,refuse-captured-parent-change}/{mutant,restored}-mirror/evidence-run1/`.
Frozen runners/inputs remain in the corresponding non-mirror directories; no
runner, OLean or full log binary is committed. The
[dated controls record](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-watcher-controls.md)
keeps this evidence separate from native qualification. These two controls do not
establish exhaustive fault coverage, parent-retry freshness, replicas, registration
writer ordering, cleanup, liveness or full native implementation correctness.
