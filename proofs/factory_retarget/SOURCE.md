# Manual correspondence, not extracted implementation verification

Source: Hub `48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c`, after integration
of served-marker preservation and active provider grants. Source hashes are retained
in the first model-check input packet. This model is not Aeneas output.

| Source | Model boundary |
| --- | --- |
| migrations 0015–0020,0032,0033,0035 | All 21 instance columns in Row; source schema validity/nullability is an interpretation condition |
| Factory.apply_to | program path, preparation observations, changed-target deletion before target commit |
| push/unserved/linked/applied/linked_applied/own_tree/client SQL | Concrete updateRow assignments, zero-row Option semantics |
| recorded/failed.sql | Original error retained; best-effort failure-write acknowledgement/commit separated |
| create/reissue_token/lore_provider | Raw token distinct from digest; full credentials/provider steps and partial effects |
| sandbox.create_request/Source.command | Computed full request/argv including attachment prefixes and structured set_path; source script, UUID/URL/default correspondence explicit |
| restart/delete_sandbox | Returned native observations separate from requested calls; confirmed absence is a native contract, not readiness |
| client_read.sql | Complete client projection from current row |
| tracking.sql + linked_pushed Rust filter | Repository SQL selection composed with tracked-ID filtering |

SQL success means atomic committed statement. SQL failure may be confirmed refusal
or unknown commit with lost reply, represented separately. Successful failed.sql
logging implies commit at interpretation; acknowledged versus committed outcomes
are distinct observations. An existing row and no concurrent removal are visible
execution-domain conditions; push RETURNING without a row is an error while other
UPDATEs can affect zero rows. There is no universal global freshness premise.

External registration/served-at writes, concurrent followers/replicas, process
nonreturn/crash and publication ordering are outside finite sequential histories.
A create error may occur before submission; createSubmittedOnError must come from
an explicit source/native observation, never from Err alone. Native post-RPC data
can be absent or in any phase. A submitted request is not proof of remote receipt,
fresh checkout or successful program execution. URL path replacement preserves
represented other components; native parser/rendering/encoding and full protobuf
remainder/default interpretation remain external. No concatenated-origin rule is
claimed for arbitrary configured URLs.

runner_applied is an explicit direct read/parse observation after readSandbox;
its successful fields correspond to the bound row under the no-concurrent-write
condition. The readSandbox observation is at the source read, not before build.
Provider refresh material includes the raw custody secret and client identifier;
client.sql stores only the native digest. Both raw/digest relations remain
explicit. Physical unrelated-native-object frames require distinct scoped
(workspace, truncated-name) keys, whereas full-deployment SQL row frames do not.

## Checked row checkpoint

The coordinator reviewed the successful model/proof logs and all guard output.
GPT-6.1 Sol authored the package; GPT-6 Luna ran the bounded Linux checks; GPT-6
Astra reviewed the correspondence and results. The proof source contains seven
printed principal closures plus 24 executable guards; every guard permits only
the three standard axioms listed below. Failed authoring attempts are preserved
and are not semantic findings. No native tests were repeated for proof-only edits.

```json
{
  "status": "checked manual full-row/result checkpoint; broader protocol partial",
  "runtime_source": "48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c",
  "lean": "4.35.0",
  "mathlib": "56a005855ea17e0b0958f73ec801ae140b0a1679",
  "aeneas_used": false,
  "model_source_sha256": "52f6da22b5acc42fdff56c6f0d32c28996f1953ce3da958a23c8fe9fbce51bf0",
  "proof_source_sha256": "ea62b2e5ef873b87fd1c5eb0b31668fb365ed2f339f5057527194c3d8988fead",
  "model": {
    "elapsed_ms": 6686,
    "exit": 0,
    "olean_bytes": 3832936,
    "olean_sha256": "86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee",
    "log_sha256": "9650a135967840a1014b97446fad4171a462ab3d3492e53f8c71291ec373b337"
  },
  "proof": {
    "elapsed_ms": 10278,
    "exit": 0,
    "olean_bytes": 4229544,
    "olean_sha256": "7a72412270c42a64c968935a531750770bb3456f7eaa92c7deb1613711aa7564",
    "log_sha256": "6a3b815a24155bd486c9de2fa55be651bfc88c1d9e60d55035b1b7eef821f30a",
    "axiom_guards": 24,
    "allowed_axioms": [
      "propext",
      "Classical.choice",
      "Quot.sound"
    ],
    "sorryAx": false
  },
  "inputs_sha256": "431af0711420fa49e2e4728f28714e56ada755bfdf7aeaced453e4e061a37084",
  "exact_seven_file_validation_map": true,
  "model_rebuilt_on_API_change": true,
  "exact_model_olean_reused_in_proof_only_retry": true,
  "logs_root": "/tmp/hub-factory-retarget-20261031/result-check2-mirror/run1/",
  "authoring_diagnostics": [
    {
      "case": "model-check1",
      "elapsed_ms": 31563,
      "exit": 1,
      "cause": "record layout, Bool-match elaboration and alternative-pattern syntax; no model artifact"
    },
    {
      "case": "model-check2",
      "elapsed_ms": 12068,
      "exit": 0,
      "scope": "earlier model elaboration only; later source/API corrections required fresh build"
    },
    {
      "case": "result-check1 proof",
      "elapsed_ms": 10458,
      "exit": 1,
      "cause": "proof tactic equations, conjunction closure, binder shadowing and List optional-index API; guards rejected unfinished auxiliaries; no accepted proof artifact"
    }
  ],
  "publication_note": "Only README/SOURCE prose changed after the exact checked seven-file snapshot; all Lean/config bytes remain validated. Native tests and runtime qualification are recorded separately."
}
```


## Checked native-output checkpoint

This extends the checked manual model without changing its source, prior row
proof or dependency pins. The independent output contracts cover the complete
modeled result and preserve path-specific retained values. They do not separately
verify every condition inside the shared lifecycle choice. The credential value
is a modeled client-query projection, not a proof of issuer/token execution.
The populated witness uses native-form raw tokens, actual SHA-256 fixture pairs,
valid IDs and explicit schema/native interpretation premises. The coordinator
independently confirmed all four digest pairs and exact source-script bytes.

```json
{
  "status": "checked manual complete-output checkpoint; broader Factory protocol partial",
  "runtime_source": "48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c",
  "proof_source_sha256": "0165725f865ae28f1d4e7e3530bec9c0d7fa088bf4a3246c83e9335c4b3bba23",
  "lakefile_sha256": "29169396ec2f0653d2098c44bf06a0eed30809b1ec6095f0beb1f9672845e196",
  "inputs_sha256": "4b4b9f9cbbf0f7966cc3c215f92115e54666503dd4b39c74a60e61de7e2e2796",
  "elapsed_ms": 6945,
  "exit": 0,
  "olean_bytes": 2460512,
  "olean_sha256": "e2c9136699ef0cc9ea71096bce41a458f0068bd9c2cc17fa42b38df0bbef04de",
  "log_bytes": 6425,
  "log_sha256": "2a24923175b3a66950c516c39c419bf4c681c523cd554474104d57e95cf9a4b0",
  "axiom_guards": 14,
  "printed_closures": 8,
  "allowed_axioms": ["propext", "Classical.choice", "Quot.sound"],
  "sorryAx": false,
  "source_pre_post_unchanged": true,
  "model_and_row_proof_oleans": "exact artifacts recorded above, verified before and after; not rebuilt",
  "local_heartbeat_limit": {"theorem": "complete_output_algebra", "limit": 800000},
  "logs_root": "/tmp/hub-factory-retarget-20261031/native-output-check5-mirror/run1/",
  "authoring_diagnostics": [
    {"case": "check1 initial launch", "exit": 127, "cause": "missing PATH; setup only"},
    {"case": "check1 actual Lean", "elapsed_ms": 30954, "exit": 1, "cause": "output algebra normalization, opaque Runner decidability and overexpanded witness projection"},
    {"case": "check2", "elapsed_ms": 19169, "exit": 1, "cause": "deterministic 200000-heartbeat timeout in broad record simplification; later missing declaration/audit failures were cascades"},
    {"case": "check3", "elapsed_ms": 7854, "exit": 1, "cause": "local option/doc-comment placement and residual restart phase-match implications"},
    {"case": "check4", "elapsed_ms": 6860, "exit": 1, "cause": "two restart implications required introducing contradictory phase/count premises"}
  ],
  "qualification_boundary": "manual SQL/async/native correspondence; no Aeneas extraction, native execution or full issuer proof",
  "publication_note": "Only README/SOURCE prose changed after the checked snapshot; Lean/config bytes unchanged. Native runtime qualification remains separate."
}
```

All failed leaf attempts emitted no accepted output artifact; executable guards
rejected incomplete dependencies. None is counted as a semantic-control result.
GPT-6.1 Sol authored the leaf, GPT-6 Luna ran bounded Linux validation and GPT-6
Astra reviewed complete logs and correspondence. No additional native checks were
run for this proof-only checkpoint. Remaining obligations include independent
choice contracts, failure-prefix/retry composition and finite sequential histories.


## Checked recovery checkpoint

`FactoryRecoveryProof` adds exact first-error prefix/trace/state results and
conditional repeated-application recovery to the same manual model. Full row,
Runner and immutable identity survive all modeled returns. The precommit segment
is proved equal to the actual changed-target program prefix before target SQL;
it includes preparation, direct reads, unserved/delete and absent-native
admission. Its errors preserve the old four-field linked target, without implying
that a native worker survived. Generic retry constructs acceptance from new
no-fault and capacity observations, including absent-native retry and unknown
prior SQL/native outcomes, then reuses the independent complete-output contract.
The failure premise on the generic retry theorem is intentionally stronger than
needed: survival holds after every return. Neither fairness nor liveness follows.
Native read/SQL/RPC interpretation and the existing sequential execution domain
remain explicit; no extracted Rust or automatic recovery claim is added.

```json
{
  "status": "checked manual first-error and conditional two-call retry checkpoint; finite histories and broader Factory protocol remain partial",
  "runtime_source": "48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c",
  "proof_source_sha256": "80f34ec4401428dbabf13e8dac061e2546974377ffc08dc0b92f695c1ebabc35",
  "lakefile_sha256": "92f2f1455d7391b3b6ded37865a4546e1e62971bfcc3fbd14230cebddf613c12",
  "inputs_sha256": "44fb74e65040c5122885533e4ab048fa16e3406956ad4d86404717017fa99335",
  "elapsed_ms": 14984,
  "exit": 0,
  "olean_bytes": 1555040,
  "olean_sha256": "30c0ee7c55eebba0d377adf80d6d3c0169d13ac22479ea553b2208424aa01e43",
  "log_bytes": 5914,
  "log_sha256": "dcf9e3e903e8ffb8278a9c688e6a823fb3fefba9513fdc2a15ba13902b19ba1a",
  "axiom_guards": 21,
  "printed_closures": 8,
  "allowed_axioms": ["propext", "Classical.choice", "Quot.sound"],
  "sorryAx": false,
  "source_pre_post_unchanged": true,
  "accepted_prerequisites": {
    "FactoryRetargetModel.olean": "86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee",
    "FactoryRetargetProof.olean": "7a72412270c42a64c968935a531750770bb3456f7eaa92c7deb1613711aa7564",
    "FactoryNativeOutputProof.olean": "e2c9136699ef0cc9ea71096bce41a458f0068bd9c2cc17fa42b38df0bbef04de"
  },
  "prerequisite_sources_unchanged": true,
  "prerequisite_oleans_reused_after_hash_gates": true,
  "local_heartbeat_limit": {"theorem": "no_fault_capacity_accepts", "limit": 600000},
  "logs_root": "/tmp/hub-factory-retarget-20261031/recovery-check3-mirror/run1/",
  "authoring_diagnostics": [
    {
      "case": "check1", "elapsed_ms": 16641, "exit": 1,
      "log_bytes": 15751,
      "log_sha256": "91fcb6767e32d92b7ddabfc5facc7a607a7050445e5eb8afc867be4e58f0308a",
      "cause": "identity binding, conditional row projections, no-ifs tactic, append association and broad no-abort simplifier heartbeat limit; cascading incomplete-dependency audit rejected"
    },
    {
      "case": "check2", "elapsed_ms": 15511, "exit": 1,
      "log_bytes": 8998,
      "log_sha256": "c553032eaecc1e600f633e241f1f3021379883fa628626f9df1b1561b16bd26d",
      "cause": "six admission membership/constructor disjunctions left in no-target helper; cascading incomplete-dependency audit rejected"
    }
  ],
  "publication_note": "Only README/SOURCE prose changed after the checked nine-file packet; all Lean/config bytes equal check3. No native tests or prior proof rebuilds were repeated."
}
```

GPT-6.1 Sol authored the leaf, GPT-6 Luna ran the bounded Linux direct check and
GPT-6 Astra reviewed the complete logs and source correspondence. Failed attempts
produced no accepted Recovery artifact; they are proof-authoring diagnostics,
not runtime findings or semantic-control results. Guards count dependency audits,
not execution scenarios. Independent lifecycle choice contracts, finite histories
and wrong-output/wrong-refusal controls remain next obligations.


## Checked choice checkpoint

`FactoryChoiceProof` imports the unchanged model alone, without prior proof leaves.
Its independent predicates spell actual target/content/configuration conditions
and establish all four iff decisions, keep precedence, rebuilt-image allowance,
provider permutation with multiplicity and ignored-field invariance. The source
mapping is `factory.rs` lifecycle dispatch (403–617) and `factory/sandbox.rs`
`runs`/`runs_beside_image` (379–418), at the recorded runtime revision. Native
sorting/permutation, protobuf/URL/UUID/script interpretation remain explicit;
this is not Aeneas extraction or concurrent-preparation freshness verification.

```json
{
  "status": "checked independent manual lifecycle contracts; broader Factory goal partial",
  "runtime_source": "48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c",
  "proof_source_sha256": "81198160608ac4d811d679e3ee4e94facd4ffd5c9be3a4e1ce70f4077bb3181a",
  "lakefile_sha256": "b687daa6627dd7b2ff8fdb5a5d7caf0032f1f48df4e83bdc6364a5714d9bbccb",
  "inputs_sha256": "dd027db7f9120296725363d524a3e5a071970a5c34702ac12af6a8dd85c8186a",
  "elapsed_ms": 5066,
  "exit": 0,
  "olean_bytes": 967736,
  "olean_sha256": "682c649e911d0e42225397aa882ace22d7905ff5c82686c3496b276d8e0b663f",
  "log_bytes": 1766,
  "log_sha256": "682838f5c94fedd9e129cdadd0f8b4ef7ed314d6417384b96569096b56f3bf13",
  "axiom_guards": 15,
  "printed_closures": 8,
  "allowed_axioms": ["propext", "Classical.choice", "Quot.sound"],
  "sorryAx": false,
  "source_pre_post_unchanged": true,
  "model_source_sha256": "52f6da22b5acc42fdff56c6f0d32c28996f1953ce3da958a23c8fe9fbce51bf0",
  "model_olean_sha256": "86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee",
  "prior_model_reused_after_hash_gates": true,
  "prior_proof_imports": false,
  "local_heartbeat_limits": {
    "choice_keep_iff": 600000,
    "choice_restart_iff": 600000,
    "choice_replace_iff": 600000
  },
  "logs_root": "/tmp/hub-factory-retarget-20261031/choice-check2-mirror/run1/",
  "authoring_diagnostics": [
    {
      "case": "check1", "elapsed_ms": 4878, "exit": 1,
      "log_bytes": 1960,
      "log_sha256": "72bb1c68681cc1a8f941415f5f00686e553cc974bda93ff9ce489f8ecc205f70",
      "cause": "nested record-update layout parser error; later unknown names were cascades; no accepted leaf artifact"
    }
  ],
  "publication_note": "Only README/SOURCE prose changed after the checked ten-file packet; all Lean/config bytes equal check2. No native/model/prior-proof rebuild was repeated."
}
```

GPT-6.1 Sol authored the leaf, GPT-6 Luna ran the bounded Linux direct check and
GPT-6 Astra reviewed the complete log, source interpretation and contracts.
The accepted result has no custom own-logic axioms or accepted unfinished proofs.
The failed attempt was a syntax diagnostic, not a runtime finding or semantic
control. Standalone populated wrong-output/wrong-refusal controls and finite
histories remain subsequent work; asynchronous follower supersession remains a
separate native hypothesis, not a consequence of these contracts.


## Standalone controls receipt

These controls mutate the **manual model** in private packages. They neither
extract nor modify Rust. Positive populated assertions import only the model and
a standalone data fixture, with explicit complete expected rows, provider/client,
request/native outputs and event traces. Independent bad-outcome witnesses check
the whole erroneous result under each identical fault. The unchanged standalone
assertion fails only after its model/fixture/witness compiled. No generic
Choice/Recovery/row-output proof artifact is reused against a mutated model.

The target fault omits only the repository/tracked-ID assignments, while command
and other fields proceed. The capacity fault changes enforced rejection from
strictly above to at-or-above the cap; count = limit = 1 should be accepted. Actual
Rust admission uses strict `instances > cap`; SQL count correspondence remains
external. The historical native retarget failure is separate implementation
evidence. Public fixture token/digest/script bytes are copied from the checked
native-output witness, but no crypto, real SQL or OpenShell execution is proved.
Native sorting and protobuf/default/formatter equality remain interpretation
conditions; the refresh strategy string denotes the native `Oauth2ClientCredentials`
enum under that correspondence, not a proof of the entire native provider object.

False-on-baseline witnesses and patches live in `mutations/`, without ordinary
baseline targets. In private validation they are copied unchanged to package root
for module resolution. Both mutations preserve the model's types/interfaces;
patch reversal and fresh own artifact graphs protect against stale proof reuse.
The separate final record is
[Factory semantic controls](https://github.com/VeoveoAI/veoveo-docs/blob/main/records/2026-10-31-factory-semantic-controls.md).

```json
{
  "status": "qualified two standalone manual-model semantic controls with exact source reversals and fresh passing baseline restorations; broader Factory goal partial",
  "runtime_source": "48d7e25a564f24c9d0418fbbebeda1d5c7d4c68c",
  "baseline_model_sha256": "52f6da22b5acc42fdff56c6f0d32c28996f1953ce3da958a23c8fe9fbce51bf0",
  "frozen_controls_inputs_sha256": "15e2b64f0b6480417429d853dea59ee665d1b643066e0f97fcfc4e15c6c624c3",
  "source_files": {
    "FactoryControlFixture.lean": "7acb8a6f1fd121067d3bd63f7abe64e4df626cebe03a3aa8397c64f8a47b18fa",
    "FactoryRetargetControl.lean": "0114680cf6adfbd2f022d2cad3e33508a6fb14bb407c1ec79d246e3376572759",
    "FactoryCapacityControl.lean": "5344d7b3342eaedf41808c466d6ae924b9c2b61dc84286561ac2d3fe83544736",
    "mutations/FactoryRetargetFaultWitness.lean": "2949380bc1e8d4bdac78cec9e73fff30b52b328d739775f312042985c35dd241",
    "mutations/FactoryCapacityFaultWitness.lean": "7be541f56526904f0035976bc7bd5cd435f7fe848b2f6e45240071fb610c8d68",
    "mutations/target-omission.patch": "f7ad4c8ded6cdd02ea72775cea44dcc922eaf817c70f8bcb4b186189935bd192",
    "mutations/capacity-at-limit.patch": "e3200b48e9b48785e2820eba305f62e8267c8cdb6dcbc02e98de2881f39e6a74"
  },
  "toolchain": {
    "lean": "4.35.0",
    "mathlib": "56a005855ea17e0b0958f73ec801ae140b0a1679",
    "aeneas_used": false,
    "kernel_checked_decide": true,
    "local_execution_theorem_maxRecDepth": 4096
  },
  "allowed_axioms": [
    "propext",
    "Classical.choice",
    "Quot.sound"
  ],
  "baseline": {
    "retarget_elapsed_ms": 22710,
    "capacity_elapsed_ms": 21644,
    "retarget_olean_sha256": "ee9b33e303f7e3903cd4bb273f9fff70341330bbf7889530f3b9ae11856dbe5d",
    "capacity_olean_sha256": "e09e0d8b142a8955a2426278a5c652d4462099223df836fc6829634631717b0a",
    "each_assertion_audit_passed": true
  },
  "target_omission": {
    "mutant_model_sha256": "8c6ad8f924b861f81e150011c30343cf1c476b50f7867dd011921050ac046c03",
    "stages": [
      {
        "stage": 1,
        "elapsed_ms": 7538,
        "exit": 0,
        "log_sha256": "306b01737e08def53600099b35a18189fed21e0340992bb82f55f76af85f7861"
      },
      {
        "stage": 2,
        "elapsed_ms": 3095,
        "exit": 0,
        "log_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      },
      {
        "stage": 3,
        "elapsed_ms": 21736,
        "exit": 0,
        "log_sha256": "c63733dd96c8530f360afb5a98bf7354adc465b336c70b3e440e42b90bb59c42"
      },
      {
        "stage": 4,
        "elapsed_ms": 3044,
        "exit": 1,
        "log_sha256": "05a066a818bc2f1ea7a89ca20a2230e0816492c62a499c943e082853b10dc002"
      }
    ],
    "witness_olean_sha256": "08921b448e50379ea94fbe347e7821f88929f164c18c1306afe66cd03e3b527b",
    "semantic_rejection": "Standalone populated full-output assertion, not a generic theorem; independent complete wrong-output witness passed first. Cascading sorryAx rejected; no failed control OLean.",
    "restoration": {
      "summary_sha256": "8b7ab4cb660e68ad33917ad7fa8ad11b14180e242a143116183a30ff96136053",
      "stage_elapsed_ms": [
        7293,
        3054,
        21294,
        21866
      ],
      "all_stages_exit_zero": true,
      "baseline_source_map_exact": true,
      "fresh_own_artifacts": true,
      "artifacts": {
        ".lake/build/lib/lean/FactoryCapacityControl.olean": {
          "sha256": "e09e0d8b142a8955a2426278a5c652d4462099223df836fc6829634631717b0a",
          "bytes": 18920
        },
        ".lake/build/lib/lean/FactoryControlFixture.olean": {
          "sha256": "0a0ce32726ec75e08608ef9af2b4ed32a6f1b2769933bbd634196030bc1f78d1",
          "bytes": 438448
        },
        ".lake/build/lib/lean/FactoryRetargetControl.olean": {
          "sha256": "ee9b33e303f7e3903cd4bb273f9fff70341330bbf7889530f3b9ae11856dbe5d",
          "bytes": 18952
        },
        ".lake/build/lib/lean/FactoryRetargetModel.olean": {
          "sha256": "86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee",
          "bytes": 3832936
        }
      },
      "source_file_count": 9,
      "all_artifact_hashes_match_baseline": true
    },
    "mutant_model_olean_sha256": "7596ddaf030a14615bad121184b253513fa90563faf86d6aa0e73a0117ad3141"
  },
  "capacity_at_limit": {
    "mutant_model_sha256": "82ad2409ab7f9e564e561094ec63a62551fdaf6c8b9453b8ed94baa737c02561",
    "stages": [
      {
        "stage": 1,
        "elapsed_ms": 8232,
        "exit": 0,
        "log_sha256": "96feecf1cc163410e5942acf518dbf6235383f4ecf73b80a4c711c9a3306bfda"
      },
      {
        "stage": 2,
        "elapsed_ms": 3881,
        "exit": 0,
        "log_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      },
      {
        "stage": 3,
        "elapsed_ms": 4836,
        "exit": 0,
        "log_sha256": "a11720167a24e6db5f85a97b5cef9ae50c047608d1ce6012caabf1cd8edd0bd1"
      },
      {
        "stage": 4,
        "elapsed_ms": 3075,
        "exit": 1,
        "log_sha256": "8a5cb19c4b544a993bc55f19ca0acebb50154c69ffc50aa66359f1d0acd244cc"
      }
    ],
    "semantic_rejection": "Standalone populated exact-cap accepted-output assertion, not a generic theorem; independent complete wrong-refusal witness passed first. Cascading sorryAx rejected; no failed control OLean.",
    "witness_olean_sha256": "56fb778cf66f45bef1ddfdd2f86602785cc934b1f001f8c665bd12529a49e863",
    "mutant_model_olean_sha256": "deabce7f391aef7758aa267c79a4b6586c36fa8db63fb40a8330d2a75049b8ff",
    "restoration": {
      "summary_sha256": "340e4984332dbcf03a135e5f59fa43418c2ddbbfd9e6db2603cb3af54035403c",
      "stage_elapsed_ms": [
        6699,
        3884,
        30345,
        20733
      ],
      "all_stages_exit_zero": true,
      "baseline_source_map_exact": true,
      "source_file_count": 9,
      "fresh_own_artifacts": true,
      "all_artifact_hashes_match_baseline": true,
      "artifacts": {
        ".lake/build/lib/lean/FactoryRetargetModel.olean": {
          "sha256": "86d35cb19aea6b19727b3bb0fc2cfcb3c3c45674a05f063c3acbd053d1f402ee",
          "bytes": 3832936,
          "present": true
        },
        ".lake/build/lib/lean/FactoryControlFixture.olean": {
          "sha256": "0a0ce32726ec75e08608ef9af2b4ed32a6f1b2769933bbd634196030bc1f78d1",
          "bytes": 438448,
          "present": true
        },
        ".lake/build/lib/lean/FactoryRetargetControl.olean": {
          "sha256": "ee9b33e303f7e3903cd4bb273f9fff70341330bbf7889530f3b9ae11856dbe5d",
          "bytes": 18952,
          "present": true
        },
        ".lake/build/lib/lean/FactoryCapacityControl.olean": {
          "sha256": "e09e0d8b142a8955a2426278a5c652d4462099223df836fc6829634631717b0a",
          "bytes": 18920,
          "present": true
        }
      }
    }
  },
  "initial_authoring_diagnostics": [
    {
      "case": "baseline1",
      "model_elapsed_ms": 7951,
      "fixture_elapsed_ms": 4339,
      "cause": "ambiguous dotted Action constructors; no positive assertion ran"
    },
    {
      "case": "baseline2",
      "fixture_elapsed_ms": 3524,
      "retarget_elapsed_ms": 3878,
      "cause": "kernel-checked decide exceeded default recursion depth; no semantic counterexample, capacity unrun"
    }
  ],
  "artifact_policy": "Own model/fixture/witness/control artifacts fresh for each mutation and restoration. Initial syntax/resource retries reused only exact gated baseline artifacts. Source exactness is recorded separately from measured artifact hashes.",
  "qualified": true,
  "qualification_scope": "Only these populated model execution assertions/witnesses, not mutated generic theorems or native Rust execution.",
  "audits": "Two baseline assertions and two separate fault witnesses each have one executable standard-only axiom guard. Restored assertions pass the same guards. Failed assertion sorryAx cascades were rejected; no correctness OLean accepted.",
  "canonical_packaging": "Validated files copied byte-exact; false-on-baseline witnesses moved under mutations without library targets. Only three positive manual targets added to existing package; no CI, source or runtime change."
}
```


Both restorations were independently reviewed, including the exact nine-file
source map and all fresh stages. The measured model, fixture and both positive
control artifacts match their baseline hashes in these runs; no general binary
reproducibility cause is asserted. The original constructor-inference and
recursion-limit failures remain setup/authoring diagnostics. The finite recursion
limit is local to each execution theorem; no heartbeat limit, evaluator bypass,
new axiom or runtime behavior was added. Native regression and concurrency claims
remain separate. GPT-6.1 Sol prepared the sources, GPT-6 Luna ran the bounded
checks, and GPT-6 Astra reviewed full logs, wrong-result witnesses and restorations.


## Checked guarded-watcher foundation

The current watcher composition model/proof, exact source/artifact/dependency
identities, model and proof authoring diagnostics, manual commands and remaining
obligations are recorded in [WATCHER_SOURCE.md](WATCHER_SOURCE.md). The inherited
Factory model and prior checked leaves remain unchanged. This foundation checks
current projection and complete accepted/no-op outputs under explicit native
mutex/SQL interpretation; it does not close the queued parent/status compositions,
constructive witnesses or selected control/restoration requirements.
