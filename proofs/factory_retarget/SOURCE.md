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
