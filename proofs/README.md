# Hub proof and evidence map

This index describes retained manual proof assets and their limits. It is not a
claim that the current Hub, SDK or deployed system is formally verified. Each
package's README and receipt identify its historical source, modeled libraries,
trusted assumptions and measured checks. Rechecking an old generated model does
not establish correspondence to a changed Rust implementation.

Broad formal-verification implementation is paused pending better Rust/Aeneas
support. The stale Work Context proof packages were removed together rather than
ported individually. No runtime code, admission policy, dependency pin, CI gate or
native regression test was changed by this proof cleanup.

## Source basis and status

Hub `496988c` and SDK `ae71ef1` removed Work Context and introduced Skills. Hub
`92ad0be` added Task exchange refusal after loss of a currently configured group.
Old membership decisions, full authority records, session predicates and Task
bindings therefore cannot stand as current-source coverage. Skills is not a rename
of Work Context membership. The new Skills and group-revocation paths remain
unproved by the retained assets.

The batch removes five Hub packages and one SDK package, including their local
models, proof code, controls and receipts (264 tracked files). Their content and
qualification history remain in Git; dated records remain in the documentation
repository. These immutable links show the complete packages before removal:

| Retired package | Pinned history | Reason for retiring the active package |
| --- | --- | --- |
| Hub `current_roles` | [Archive](https://github.com/VeoveoAI/veoveo-hub/tree/b5c08321d752c9e42a817b076fe82b2d53900619/proofs/current_roles) | Deleted membership target and shared records/bridges that retained Work Context |
| Hub `control_publication` | [Archive](https://github.com/VeoveoAI/veoveo-hub/tree/b5c08321d752c9e42a817b076fe82b2d53900619/proofs/control_publication) | Removed Work Context rows, grants, memberships and publication audit program |
| Hub `refresh_transactions` | [Archive](https://github.com/VeoveoAI/veoveo-hub/tree/b5c08321d752c9e42a817b076fe82b2d53900619/proofs/refresh_transactions) | Changed stored family/session payload; dependency on the retired shared graph |
| Hub `request_composition` | [Archive](https://github.com/VeoveoAI/veoveo-hub/tree/b5c08321d752c9e42a817b076fe82b2d53900619/proofs/request_composition) | Changed complete authority/token records and dependency on the retired shared graph |
| Hub `task_continuity` | [Archive](https://github.com/VeoveoAI/veoveo-hub/tree/b5c08321d752c9e42a817b076fe82b2d53900619/proofs/task_continuity) | Removed concrete binding field; new outer group-revocation condition not covered |
| SDK `internal_auth` | [Archive](https://github.com/VeoveoAI/veoveo-hub-sdk/tree/a85b72d37bb8e8a90c5c46ed6f1984aae7da9250/proofs/internal_auth) | Removed context equality/authority fields and changed current issuer graph |

Retiring a package does not disprove its historical theorems. Some generic lemmas
could be reused in a future bounded effort; they are not maintained as active
coverage through stale generated types. The unfinished membership controls were
never qualified. Their [closeout](https://github.com/VeoveoAI/veoveo-hub/tree/b5c08321d752c9e42a817b076fe82b2d53900619/proofs/current_roles/evidence/work-context-membership-closeout.json)
records Lean diagnostic-authoring failures, exact private source restoration and a
passing original audit; it does not establish semantic fault detection.

## Retained proof packages

These eight packages have no Lean import or Lake dependency on a retired package.
Their recorded results keep the original source/refinement boundaries. They have
not become current whole-system proofs through this cleanup.

| Package | Recorded contract | Important boundary |
| --- | --- | --- |
| [Hub authority](authority/README.md) | Exact staged permission exchange and downstream narrowing; valid outcomes and refusal order | Generic set/resource/current-policy laws and async caller freshness |
| [Hub selection](selection/README.md) | Exact explicit/base selection and zero/one/multiple matching | Native lookup, snapshot acquisition and caller integration |
| [Hub discovery cache](discovery_cache/README.md) | Synchronous cache effects, complete key/state contracts and supported finite histories | Fingerprint production, native collections/time, async watchers and new Skills wrapper |
| [Hub Factory](factory_retarget/README.md) | Manual complete-output lifecycle, retarget, failure/retry and watcher/saved-parent histories | Allocation/retirement, SQL/native effects and distributed recovery |
| [Hub registration transactions](registration_transactions/README.md) | Manual registration/update/removal histories, audit programs and scoped postcommit continuation | Actual SQL/Rust/Core refinement, remaining operation adapters and complete response construction |
| [Hub Computer lifecycle](computer_lifecycle/README.md) | Manual claim/finish/fail effects, ordered worker outcomes and stale completion preservation | Native cancellation, outer timeout scheduling, browser admission and credential custody |
| [SDK authorization](https://github.com/VeoveoAI/veoveo-hub-sdk/tree/main/proofs/authorization) | Generic resource/template resolution, exact precedence, matching and conflicts | Native parsing/representation and new Skills ownership/dispatch |
| [SDK repository details](https://github.com/VeoveoAI/veoveo-hub-sdk/tree/main/proofs/repository_grants) | Full repository/action construction, projection and roundtrip, including empty read grants | Assignment production, token issue/verification, serialization and native collection refinement |

Native regression tests and production fixes remain in place. They are separate
component evidence, not formal SQL refinement or deployed/model-driven acceptance.
Some retained proofs were checked with a qualified private Aeneas patch; their
READMEs link its immutable historical bytes. Dependency pins are unchanged.

<a id="approved-implementation-roadmap"></a>

## Priorities

The invariant IDs and section anchors below are retained for links from dated
records. They are coverage categories, not an approved implementation queue or a
completion percentage. No end-to-end property is claimed complete.

## HUB-CONTRACT-01 — Shared extension contracts

Retained SDK authorization proofs cover the bounded resolver/matcher contract.
Declaration generation, wire advertisement, registration validation and dispatch
composition remain open, including the newly introduced Skills contract.

## HUB-TASK-01 — Captured Task routing and retry integrity

The active Task proof package is retired. Generic capture, ownership and retry
ideas remain in pinned history, but the old Work Context binding is gone and
current configured-group revocation has no retained proof. Native Task tests are
unchanged; SQL/lifecycle and current async admission are not established here.

## HUB-AUTH-01 — Current policy and delegated authority

Retained [authority proofs](authority/README.md) establish staged generic exchange
and permission narrowing. Their permission-set membership is not Work Context
membership. Current catalog projection, tenant/profile authority assembly, issuer
and recipient continuity need a new explicit source connection.

## HUB-OAUTH-01 — Session binding and refresh lifecycle

Session/current-role and refresh-family proof packages are retired. Their exact
old predicates and payloads are not current session coverage. Token cryptography,
store lookup, refresh lifecycle, revocation and cleanup composition remain outside
the active retained proofs.

## HUB-CONFIG-01 — Coherent atomic live publication

The publication model is retired because the represented Work Context row/audit
program was removed. CAS, transaction atomicity and replay invariants remain
historical results, not a proof of the replacement tenant-publication program or
replica convergence.

## HUB-ROUTE-01 — Fresh routing and captured routing

Retained [selection proofs](selection/README.md) cover the exact resolver kernel.
The larger request/issuer composition package is retired. Fresh snapshot/TTL
logic, complete current authority assembly and actual async routing remain open.

## HUB-CATALOG-01 — Request/catalog/cache/listen isolation

Retained [cache proofs](discovery_cache/README.md) cover their recorded synchronous
algorithm and supported histories. The generic key/algorithm was not removed with
Work Context. Current fingerprint-input correspondence, Skills discovery, full
pagination, async watch/reconnect behavior and cross-replica freshness remain open.

## HUB-FACTORY-01 — Reconciliation and native lifecycle recovery

Retained [Factory models/proofs](factory_retarget/README.md) have complete modeled
row/client/native/provider/request outputs and scoped recovery histories.
Allocation, full membership retirement, native/OpenShell correspondence and
cross-replica eventual recovery remain open. Fake-provider fixtures are synthetic.

## HUB-COMPUTER-01 — Owner/session identity and version recovery

Retained [Computer lifecycle proofs](computer_lifecycle/README.md) cover the manual
replacement protocol and durable stale-completion guard. They do not establish
remote cancellation, native side-effect fencing or browser/custody recovery.
The deleted Work Context membership calculation supplies no current admission
coverage.

## HUB-REGISTER-01 — Registration and deployment identity

Retained [transaction proofs](registration_transactions/README.md) cover their
recorded manual registration/update/removal/audit protocols, constructive outcomes
and supported failure/commit histories. Source/SQL/PG correspondence, complete
current payload inputs, remaining adapters and native response construction remain
explicit obligations. An error reply does not imply rollback.

## HUB-LORE-01 — Native grants and contributed token issuance

Retained SDK repository-detail proofs preserve the complete repository ID/action
set at that helper boundary. Assignment/group union, current issuer/verifier and
native Lore authorization continuity remain unproved. No Work Context membership
proof is a substitute for repository authority.

## HUB-CLEANUP-01 — Cleanup, references and transactional audit

Retained registration models provide scoped login/data/audit cleanup and rollback
contracts. Protected references across other resources, session/Task retention,
external deletion and concurrent cleanup remain open. SQL atomicity does not make
remote side effects atomic.

## Existing formal evidence and its limits

Extracted-function proofs, handwritten protocol models and native tests are distinct
evidence. Translator/compiler correctness, ordinary library models, physical value
representation and PostgreSQL contracts remain explicit trust/refinement boundaries.
Deliberate source mutants demonstrate sensitivity only when independently qualified;
authoring/setup failures do not count as semantic detection.

The six removed packages and their tool/byte receipts remain available at the
pinned history links above. Immutable dated qualification records are not rewritten
as present-day guarantees. No broad current-source theorem or deployment result is
inferred from an old passing build.

## Manual updates and reuse

The retained package READMEs describe their recorded manual commands. There is no
new automatic gate or requirement to run a proof campaign for ordinary development.
Any future verification effort needs explicit authorization, a bounded property,
source identity and completion criterion. This cleanup does not restart that work.
