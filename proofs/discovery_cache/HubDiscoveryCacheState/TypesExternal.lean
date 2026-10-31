-- Handwritten external primitive models based on generated interfaces.
-- Native representation/refinement boundaries are documented with each model.
module
public import Aeneas
@[expose] public section
open Aeneas Aeneas.Std Result ControlFlow Error
set_option linter.dupNamespace false
set_option linter.hashCommand false
set_option linter.unusedVariables false
set_option linter.style.whitespace false
set_option linter.style.setOption false
set_option linter.style.longLine false
set_option linter.internalConstructors false

/- You can set the `maxHeartbeats` value with the `-max-heartbeats` CLI option -/
set_option maxHeartbeats 1000000

/- You can set the `maxRecDepth` value with the `-max-recdepth` CLI option -/
set_option maxRecDepth 2048

namespace HubDiscoveryCacheModel

/-- [core::time::Duration]
    Source: '/rustc/library/core/src/time.rs', lines 81:0-81:19
    Name pattern: [core::time::Duration]
    Visibility: public -/
@[rust_type "core::time::Duration"]
structure core.time.Duration where
  nanoseconds : Nat
  deriving DecidableEq

/-- [alloc::collections::btree::map::BTreeMap]
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 189:0-193:1
    Name pattern: [alloc::collections::btree::map::BTreeMap]
    Visibility: public -/
@[rust_type "alloc::collections::btree::map::BTreeMap"]
abbrev alloc.collections.btree.map.BTreeMap (K : Type _) (V : Type _) (A : Type _) :=
  List (K × V)

/-- [tokio::time::instant::Instant]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/time/instant.rs', lines 34:0-34:18
    Name pattern: [tokio::time::instant::Instant]
    Visibility: public -/
@[rust_type "tokio::time::instant::Instant"]
structure tokio.time.instant.Instant where
  nanoseconds : Int
  deriving DecidableEq

/-- [uuid::Uuid]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/uuid-1.25.0/src/lib.rs', lines 467:0-467:15
    Name pattern: [uuid::Uuid]
    Visibility: public -/
@[rust_type "uuid::Uuid"]
structure uuid.Uuid where
  high : Std.U64
  low : Std.U64
  deriving DecidableEq

end HubDiscoveryCacheModel
