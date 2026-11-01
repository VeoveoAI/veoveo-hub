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
