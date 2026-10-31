# Discovery cache state proofs

This package models the actual five synchronous `SurfaceState` operations extracted
from Hub `c23d9ae`, with SDK `7360c0f` and Core `422dcf2`.
`HubDiscoveryCacheState/Types.lean` and `Funs.lean` are unchanged generated files.
The [current qualification](evidence/current-linked-cache-proofs-20261031.json)
checks four model modules, all nine proof modules and 103 executable axiom guards.
It extends the earlier `688b91a` graph with the optional link and actual identifier
wrapper derives. Earlier receipts and timings below remain evidence for their named
source. The [link-order control](evidence/link-order-control-20261031.json) now
qualifies the new key distinction with an actual Rust fault and exact restoration.

The control keeps all key fields but deliberately makes `Ord` ignore the link's
comparison result. The unchanged native regression observes one link's complete
descriptor replacing another's and invalidation losing the other's pending result.
A fresh generated model compiles; an independent Funs-only witness proves unequal
present-link keys compare equal. The unchanged `key_comparison_exact` rejects that
result. Its failed OLean is absent; cascading audit errors are not counted as
additional failures. The first witness attempt had two unavailable lemma names;
only its tactics were corrected, without changing statements, guards or models.
Same-tree reversal restores all 1,144 source files and all four generated files
exactly, then a fresh model and all nine original proof modules pass. The receipt
retains both attempts, exact commands, output identities and the native assertion.
This is a deliberate fault control, not a new production bug or deployment.

The first proof leaf covers complete cached reads, cached-only removal, and full key
invalidation. Its independent observations retain entire payloads and denied counts.
The clone premise applies to the supplied payload `Clone` dictionary; it is not an
assumed successful cache operation. Cached-only removal preserves tickets and retry
records and therefore does not fence an in-flight completion. Full key invalidation
removes all three records at that key while preserving generation and unrelated keys.
The frame lemma describes the independent erased maps; combining it with
`discard_key_exact` connects those observations to the actual extracted operation.
The actual-operation `discard_key_link_isolation` derives key inequality from
different link fields and preserves every other link's complete cached value,
ticket and retry record. `discard_key_none_some_isolation` also covers an absent
link versus every present link, including a represented empty string. No parse-validity
or successful-operation premise is assumed. Writes at capacity may still evict
another key; the link theorem concerns full-key invalidation.

The handwritten external model uses ordered, unique-key association lists for native
`BTreeMap` values. Native refinement requires coherent borrowing/equality/order and
representable lengths. `pop_first` corresponds to minimum-key eviction only on an
appropriately ordered list. Scalar, UUID, UTF-8 ordering,
allocation, and destructor correspondence are explicit native boundaries. UUIDs have
128 bits. PrincipalId, ServerSlug and LinkName use their actual generated String-backed
representations and derives, with concrete external String/Option operations.
No hash
injectivity assumption or clock/request-ticket freshness axiom is introduced.

Time has nanosecond resolution. Duration construction multiplies milliseconds by
1,000,000. Native `Instant` addition retains an explicit `Result` boundary, including
possible overflow/failure. Async mutex acquisition and the connection from native
clock/UUID function items to operation-specific callback observations remain outside
the extracted synchronous operations.

The actual `start` operation also has an independent full-state contract in
`CacheStartSpecification` and `CacheStartProof`. Old-generation work returns without
changing any state, independently of both callback results. Strictly newer
generation clears all three maps and then retains the ticket callback's exact
Result. Equal-generation coalesced work checks a stored retry deadline before
pending work and capacity. Capacity refuses even a non-coalesced replacement of an
existing key. Accepted work returns the complete original key, observed ticket and
`denied = 0`, preserves cached/retry maps and unrelated tickets, and has at most
4096 in-flight entries. The non-coalesced acceptance specialization requires no
clock observation or condition on stored retry/pending records.

Clock and ticket Results are operation-specific inputs, including failure and
divergence. The equation establishes dependency ordering and branch independence;
native invocation counts, effects and mutex linearization remain source-reviewed
boundaries. No post-panic native state or globally fresh no-argument UUID is assumed.
The only start input bound is physical list/usize correspondence, waived for the
old map when newer generation first clears it.

The actual `complete` operation now has an independent full-state contract in
`CacheCompleteSpecification` and `CacheCompleteProof`. A missing or wrong ticket
returns `false` with every state field unchanged, without requiring a normal clock
or element-comparison result. A current successful response installs the complete
vector and denied count, removes its ticket and retry record, and frames unrelated
values except the explicitly named capacity-eviction victim. A current failed
response retains cached entries, removes its ticket, and records the observed retry
deadline after the native clock/addition bind. The actual SDK TTL initializer is
proved to yield exactly five seconds in the nanosecond duration model.

For arbitrary payload `PartialEq`, the changed flag preserves the pinned library's
`Vec.ne` Result, including per-element `.ne`, failures and divergence. It is not
replaced with `!eq`, structural inequality or a reflexivity assumption; native Vec
specialization/equality correspondence remains a boundary. A proved false-flag case
still consumes the current ticket and retry, so `false` alone does not mean no-op.

The direct composition theorem executes actual `discard_key` followed by actual
`complete`. With unique in-flight keys it returns precisely the invalidated state,
for either a successful or failed late payload and arbitrary clock input. Its missing
ticket is derived from key erasure, not assumed. This covers supported key invalidation,
not the unsupported server-wide operation below.

`CacheKeyOrderProof` now connects the actual extracted comparator to a lossless
lexicographic code of every key field and fingerprint byte. Its strict-order laws
use the library laws for that exact comparator. `CacheMapInvariantProof` derives
unique keys from strict ordering and proves ordering preservation through insertion,
removal and minimum removal. Insertion returns the previous full value, preserves
unrelated values, and grows by exactly one only for an absent key. The combined
room/insertion operation preserves the 4096 bound, and the named eviction victim is
the minimum key. These are contracts of the concrete external collection model;
they do not verify Rust's standard-library `BTreeMap` implementation.

`CacheStateInvariantProof` establishes those invariants for the empty represented
state and preserves them through actual `start`, `complete`, `remove_cached` and
`discard_key` normal-return equations. Every map stays strictly ordered, unique and
bounded by 4096, and all stored key generations equal the state's generation.
Start never decreases generation; the other mutations preserve it exactly. The
physical usize premises used by earlier contracts follow from these bounds, without
assuming a particular host width. No desired after-state invariant is assumed.

Matching an in-flight ticket now derives its key's generation. Consequently any
older-generation completion is a complete no-op, independently of ticket freshness,
payload comparison, denied count or clock result. The initial empty representation
is structural; native constructor and mutex correspondence remain external. Finite
actual-operation histories and same-key replacement fencing are established below.

`CacheHistoryProof` defines steps by the actual extracted methods' normal-return
equations, including cached-read observations that leave state unchanged. It does
not copy an executor or put `StateOk` into the step/history premises. Induction from
the empty representation derives all map invariants and nondecreasing generation
across arbitrary finite supported histories. Once generation advances past a fetch,
any later supported suffix still leaves that fetch's completion a whole-state no-op.

After actual full key invalidation, a finite suffix also fences the old completion
when accepted starts for that same key do not reuse its ticket. This `Nonreuse`
condition applies only to accepted outputs: rejected starts, unused callback inputs,
other keys and other operations remain unrestricted. Final ticket exclusion is
derived rather than assumed. Constructive acceptance lemmas build an initial
noncoalesced history and append a successful generation advance to an existing
history, with the exact observed ticket and zero denied count. Neither needs a clock
success premise. Cached-only removal remains distinct from full key invalidation.

Server-wide `discard_server` invalidation remains unsupported and unproved. The
six-root extraction translated, but unchanged generated retain-closure dictionaries
failed Lean elaboration because mutable value outputs do not fit the pinned
`FnOnce`/`FnMut` interfaces. The intermediate five-root graph excludes that operation
and its retain callbacks; it does not replace them with an opaque transition or
close the full six-operation/history objective.

The package has one nondefault manual model target. The semantic leaf is checked
explicitly after that model target:

```sh
LAKE_NO_UPDATE=1 lake build HubDiscoveryCacheState
LAKE_NO_UPDATE=1 lake env lean CacheKeyFrameProof.lean -o .lake/build/lib/lean/CacheKeyFrameProof.olean
LAKE_NO_UPDATE=1 lake env lean CacheStartSpecification.lean -o .lake/build/lib/lean/CacheStartSpecification.olean
LAKE_NO_UPDATE=1 lake env lean CacheStartProof.lean -o .lake/build/lib/lean/CacheStartProof.olean
LAKE_NO_UPDATE=1 lake env lean CacheCompleteSpecification.lean -o .lake/build/lib/lean/CacheCompleteSpecification.olean
LAKE_NO_UPDATE=1 lake env lean CacheCompleteProof.lean -o .lake/build/lib/lean/CacheCompleteProof.olean
LAKE_NO_UPDATE=1 lake env lean CacheKeyOrderProof.lean -o .lake/build/lib/lean/CacheKeyOrderProof.olean
LAKE_NO_UPDATE=1 lake env lean CacheMapInvariantProof.lean -o .lake/build/lib/lean/CacheMapInvariantProof.olean
LAKE_NO_UPDATE=1 lake env lean CacheStateInvariantProof.lean -o .lake/build/lib/lean/CacheStateInvariantProof.olean
LAKE_NO_UPDATE=1 lake env lean CacheHistoryProof.lean -o .lake/build/lib/lean/CacheHistoryProof.olean
```

The primitive model build passed in 10.685s. The final read/key-invalidation leaf
passed in 4.230s, including ten executable axiom guards and five printed theorem
closures containing only `propext`, `Classical.choice`, and `Quot.sound`. Earlier
proof-authoring failures are retained separately. These results qualify the stated
model-relative read/key-invalidation contracts. The independent start specification
compiled in 12.227s; the final start leaf passed in 7.398s, with sixteen executable
axiom guards and eleven printed closures containing only the same three standard
axioms. One proof-statement parenthesization correction restored the intended
non-coalesced acceptance domain; it changed no runtime behavior. Initial script
precheck errors and proof-authoring failures are recorded separately from successful
checks. The completion specification compiled in 14.361s and its proof passed in
4.474s on the first compiler check. Thirteen executable guards passed: six
primitive/frame declarations permit only standard axioms, and seven actual-completion
outcome declarations additionally permit only the existing native
`HubDiscoveryCacheModel.tokio.time.instant.Instant.Insts.CoreOpsArithAddDurationInstant.add`
axiom. Of nine printed closures, two are standard-only and seven include that exact
addition boundary; none contain `sorryAx`. The dependency appears even in stale and
success theorem closures because Lean traverses the whole referenced completion
definition; it does not assert time addition occurs in those branches. Native addition
is not assumed total, nonoverflowing or future-valued. Those completion results alone
do not qualify history/map invariants or native collection/time refinement.

The key-order leaf subsequently passed in 12.449s, and the final map-invariant leaf
passed in 2.533s while reusing that exact checked key-order artifact. Their eleven
and twenty-one executable axiom guards accept only the three standard logical
axioms; all twelve printed closures match, with no `sorryAx`. The first key check
failed because two library order instances did not match; the repair uses laws for
the exact comparator. The first map check then found one missing definition unfold
in the removal proof. Both proof-authoring failures are retained in the
[key/map receipt](evidence/key-map-invariants-20261031.json). Neither was a runtime
defect or required a changed map contract. Source/configuration and imported artifacts
were checked around each run. No native, generated, primitive-model or tool source
changed, and no prior proof/model dependency was rebuilt.

The [state-invariant leaf](evidence/state-invariants-20261031.json) passed in 2.776s.
Seventeen executable guards passed: fourteen allow standard logical axioms only;
three completion-related declarations additionally allow precisely the existing
native `Instant.add` Result boundary. Five printed closures are standard-only and
two include that boundary, with no `sorryAx`. The first attempt failed on an
unreduced record projection and a nonexistent library lemma name; proof-only repairs
preserved all statements. Sixteen source/configuration entries and eleven prerequisite
artifacts were checked around the accepted run. This is normal-return closure;
native post-panic state, callback invocation counts and liveness are not claimed.

The [finite-history leaf](evidence/finite-histories-20261031.json) passed in 2.847s.
All twelve executable guards pass: `start_excludes` is standard-only; eleven
history/completion declarations additionally permit precisely the existing native
`Instant.add` boundary. All seven printed history closures include that boundary,
including constructive start histories: their `History`/`Step` types contain the
actual completion equation. This dependency does not mean addition executes during
a start. Earlier parser, projection and induction-index errors are recorded as failed
authoring attempts; no `sorryAx` appears in accepted closures. Seventeen inputs and
twelve prerequisite artifacts were gated around the final check. Actual-source
semantic controls are qualified below; server-wide invalidation and
listener freshness remain open.

Two [actual-source semantic controls](evidence/semantic-controls-20261031.json)
are qualified in isolated private source/build trees. Native times below cover whole
Cargo commands including compilation; test bodies take about 0.00–0.01s, so these
are not cache-operation latencies. These deliberately inserted
faults are not new production defects. The accepted runtime, generated files,
external primitive models and all nine proof leaves remain unchanged.

The [generation-clear fault](mutations/generation-clear.patch) deletes only the
newer-generation in-flight clear. The unchanged native test fails at the intended
old-cache absence assertion in 33.092s and passes restored in 10.829s. Its fresh
model (9.791s), KeyFrame (2.827s) and StartSpecification (2.582s) compile; unchanged
`start_exact` then fails in 2.926s on retained old in-flight state instead of a
reset/singleton. The audit rejects cascading `sorryAx`, and no failed-target OLean
exists. Exact source/raw restoration and a new baseline build pass: model 9.448s,
all nine proof leaves 2.528–3.082s each.

The original [key-presence candidate](mutations/ticket-presence.patch) also fails
its native old-cache assertion (31.525s) and passes restored (12.078s), but its
extraction prunes Option/UUID equality dictionaries named by unchanged
`ticket_guard_exact`. Source review rejected it as a suitable formal control;
there is no claimed Lean run or missing-name semantic failure. Its full map/raw4
also restore exactly. Its nonfatal unused-request warning is recorded separately.

The qualified [ticket fault](mutations/ticket-equality-retained.patch) retains the
comparison and conjoins key absence, preserving that dependency while admitting a
wrong present ticket. Its unchanged native test fails in 31.325s and passes restored
in 9.995s. The fresh model (8.841s), KeyFrame (2.676s), StartSpecification (2.630s),
StartProof (2.981s) and CompleteSpecification (2.629s) compile. Unchanged
`complete_exact` fails in 2.984s: its stale-case complete no-op conflicts with the
mutant's key-present consume/write path. `ticket_guard_exact` remains standard-only;
there is no missing-name/type error or failed-target OLean. The audit rejects the
failed declaration's cascading `sorryAx`. Exact source/raw restoration and another
fresh baseline build pass: model 8.990s, all nine proof leaves 2.586–3.130s each.

Every baseline restoration verifies the original 1083-file map and all four accepted
raw files byte-for-byte, including templates/metadata. Both restored packages start
without own build artifacts and rebuild all thirteen modules. All nine proof OLean
hashes match the earlier accepted artifacts; the alternate's nine proof logs also
match the generation baseline. The four model OLeans are recorded with measured
fresh bytes/hashes; binary identity or a cause for their differences is not claimed.
The existing exact `Instant.add` boundary is unchanged. Native assertions, semantic
proof residuals and the rejected interface-pruning candidate are distinct evidence.

The native ticket fixture invalidates by server, while the formal history fence
uses supported key invalidation. Server-wide retain remains unproved. Each native
mutant aborts at the first old-cache assertion; later replacement-completion
assertions are not claimed as observed. These controls qualify sensitivity of the
stated synchronous contracts, not native async/mutex refinement, listener freshness
or the full catalog/cursor goal.

Validation used Lean 4.35.0 and the pinned Aeneas revision with the existing
[qualified discriminant instance-name patch](../request_composition/tooling/qualified-discriminant-instance.patch)
in a recorded private path override. This cache graph has no new generated enum or
`read_discriminant` reference, and no direct need for that patch was established.
Clean upstream reproduction has not been measured; the canonical manifest pin is
not a claim that an unmodified upstream dependency was tested. No new cache tool
patch was introduced.

Extraction and unchanged-raw interface diagnostics are separate evidence. Passing
raw-interface elaboration alone is not a semantic proof.

The subsequent [native watch qualification](evidence/native-watch-publication-20261031.json)
corrects a real caller defect outside these synchronous methods. An admitted
handler's selected catalog is immutable; discovery watches had also used its
private publication channel. Three regressions reproduce stale streams/cache,
late completion after publication, and an old admission removing a newer watch.
The fix uses the original live catalog for epoch checks and subscription, including
a check after waiting for the watch-map mutex, while keeping selected routing.
The same tests then pass, together with 20 nearby tests, formatting and strict
library Clippy. The tests observe actual upstream listen-future disposal and retain
complete descriptor/denied-count assertions. The cached state source is unchanged.
This adds native adapter evidence; it does not prove arbitrary scheduling, channel
delivery, reconnect behavior, or the full asynchronous watch operation in Lean.
