# Discovery cache state proofs

This package models the actual five synchronous `SurfaceState` operations extracted
from Hub `688b91a` (the same source bytes as the qualified `f81159f` archive plus its
reviewed lazy-input/name overlay), with SDK `154a473` and Core `120306e`.
`HubDiscoveryCacheState/Types.lean` and `Funs.lean` are unchanged generated files.

The first proof leaf covers complete cached reads, cached-only removal, and full key
invalidation. Its independent observations retain entire payloads and denied counts.
The clone premise applies to the supplied payload `Clone` dictionary; it is not an
assumed successful cache operation. Cached-only removal preserves tickets and retry
records and therefore does not fence an in-flight completion. Full key invalidation
removes all three records at that key while preserving generation and unrelated keys.
The frame lemma describes the independent erased maps; combining it with
`discard_key_exact` connects those observations to the actual extracted operation.

The handwritten external model uses ordered, unique-key association lists for native
`BTreeMap` values. Native refinement requires coherent borrowing/equality/order and
representable lengths. `pop_first` corresponds to minimum-key eviction only on an
appropriately ordered list. Scalar, nominal identifier, UUID, UTF-8 ordering,
allocation, and destructor correspondence are explicit native boundaries. UUIDs have
128 bits; principals and server names have distinct nominal string domains. No hash
injectivity assumption or clock/request-ticket freshness axiom is introduced.

Time has nanosecond resolution. Duration construction multiplies milliseconds by
1,000,000. Native `Instant` addition retains an explicit `Result` boundary, including
possible overflow/failure. Async mutex acquisition and the connection from native
clock/UUID function items to operation-specific callback observations remain outside
the extracted synchronous operations.

Start/complete full-state contracts and finite histories over the actual method
results are the next proof step. Payload equality must follow the supplied
`PartialEq` dictionary or an explicit full-value law. A completion returning `false`
can still remove current ticket/retry records; it does not mean state was unchanged.

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
```

The primitive model build passed in 10.685s. The final read/key-invalidation leaf
passed in 4.230s, including ten executable axiom guards and five printed theorem
closures containing only `propext`, `Classical.choice`, and `Quot.sound`. Earlier
proof-authoring failures are retained separately. These results qualify the stated
model-relative read/key-invalidation contracts, not start/complete histories or
native collection/time refinement.

Validation used Lean 4.35.0 and the pinned Aeneas revision with the existing
[qualified discriminant instance-name patch](../request_composition/tooling/qualified-discriminant-instance.patch)
in a recorded private path override. This cache graph has no new generated enum or
`read_discriminant` reference, and no direct need for that patch was established.
Clean upstream reproduction has not been measured; the canonical manifest pin is
not a claim that an unmodified upstream dependency was tested. No new cache tool
patch was introduced.

Extraction and unchanged-raw interface diagnostics are separate evidence. Passing
raw-interface elaboration alone is not a semantic proof.
