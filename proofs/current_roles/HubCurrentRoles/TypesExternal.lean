-- Handwritten external type models for the unchanged Hub extraction.
-- Native representation/refinement is an explicit proof boundary.
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


/- Arbitrary, unbounded nominal values. Validation/decoding of the Rust newtypes
is outside this slice. Different tags preserve distinct ID domains. -/
structure CurrentRolesId (tag : Nat) where
  value : _root_.String
  deriving DecidableEq

/-- [core::iter::adapters::filter::Filter]
    Source: '/rustc/library/core/src/iter/adapters/filter.rs', lines 21:0-21:23
    Name pattern: [core::iter::adapters::filter::Filter]
    Visibility: public -/
@[rust_type "core::iter::adapters::filter::Filter"]
structure core.iter.adapters.filter.Filter (I : Type _) (P : Type _) where
  iter : I
  predicate : P

/-- [alloc::collections::btree::map::BTreeMap]
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 189:0-193:1
    Name pattern: [alloc::collections::btree::map::BTreeMap]
    Visibility: public -/
@[rust_type "alloc::collections::btree::map::BTreeMap"]
abbrev alloc.collections.btree.map.BTreeMap (K : Type _) (V : Type _) (A : Type _) := List (K × V)

/-- [alloc::collections::btree::set::BTreeSet]
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 78:0-81:1
    Name pattern: [alloc::collections::btree::set::BTreeSet]
    Visibility: public -/
@[rust_type "alloc::collections::btree::set::BTreeSet"]
abbrev alloc.collections.btree.set.BTreeSet (T : Type _) (A : Type _) := Finset T

/-- [alloc::collections::btree::set::Iter]
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 135:0-135:26
    Name pattern: [alloc::collections::btree::set::Iter]
    Visibility: public -/
@[rust_type "alloc::collections::btree::set::Iter"]
abbrev alloc.collections.btree.set.Iter (T : Type _) := List T

/-- [chrono::datetime::DateTime]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chrono-0.4.45/src/datetime/mod.rs', lines 56:0-56:33
    Name pattern: [chrono::datetime::DateTime]
    Visibility: public -/
@[rust_type "chrono::datetime::DateTime"]
abbrev chrono.datetime.DateTime (Tz : Type _) (Offset : Type _) := Int × Nat

/-- [veoveo_types::identity::GroupId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 26:0-26:18
    Name pattern: [veoveo_types::identity::GroupId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::GroupId"]
abbrev veoveo_types.identity.GroupId := CurrentRolesId 0

/-- [veoveo_types::names::ScopeName]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/names.rs', lines 5:0-5:20
    Name pattern: [veoveo_types::names::ScopeName]
    Visibility: public -/
@[rust_type "veoveo_types::names::ScopeName"]
abbrev veoveo_types.names.ScopeName := CurrentRolesId 1

/-- [veoveo_types::identity::TokenSubject]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 118:0-118:23
    Name pattern: [veoveo_types::identity::TokenSubject]
    Visibility: public -/
@[rust_type "veoveo_types::identity::TokenSubject"]
abbrev veoveo_types.identity.TokenSubject := CurrentRolesId 2

/-- [veoveo_types::identity::TokenIssuer]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 115:0-115:22
    Name pattern: [veoveo_types::identity::TokenIssuer]
    Visibility: public -/
@[rust_type "veoveo_types::identity::TokenIssuer"]
abbrev veoveo_types.identity.TokenIssuer := CurrentRolesId 3

/-- [veoveo_types::identity::RoleId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 30:0-30:17
    Name pattern: [veoveo_types::identity::RoleId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::RoleId"]
abbrev veoveo_types.identity.RoleId := CurrentRolesId 4

/-- [veoveo_types::identity::TenantId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 14:0-14:19
    Name pattern: [veoveo_types::identity::TenantId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::TenantId"]
abbrev veoveo_types.identity.TenantId := CurrentRolesId 5

/-- [veoveo_types::identity::PrincipalId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 10:0-10:22
    Name pattern: [veoveo_types::identity::PrincipalId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::PrincipalId"]
abbrev veoveo_types.identity.PrincipalId := CurrentRolesId 6

/-- [veoveo_types::identity::DataLabelId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 6:0-6:22
    Name pattern: [veoveo_types::identity::DataLabelId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::DataLabelId"]
abbrev veoveo_types.identity.DataLabelId := CurrentRolesId 7

/-- [veoveo_hub_sdk::identity::LocalToolName]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 172:0-172:24
    Name pattern: [veoveo_hub_sdk::identity::LocalToolName]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::LocalToolName"]
abbrev veoveo_hub_sdk.identity.LocalToolName := CurrentRolesId 8

/-- [veoveo_hub_sdk::identity::PromptName]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 176:0-176:21
    Name pattern: [veoveo_hub_sdk::identity::PromptName]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::PromptName"]
abbrev veoveo_hub_sdk.identity.PromptName := CurrentRolesId 9

/-- [veoveo_types::permission_names::ResourceTypeName]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/permission_names.rs', lines 10:0-10:27
    Name pattern: [veoveo_types::permission_names::ResourceTypeName]
    Visibility: public -/
@[rust_type "veoveo_types::permission_names::ResourceTypeName"]
abbrev veoveo_types.permission_names.ResourceTypeName := CurrentRolesId 10

/-- [veoveo_types::permission_names::PermissionName]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/permission_names.rs', lines 7:0-7:25
    Name pattern: [veoveo_types::permission_names::PermissionName]
    Visibility: public -/
@[rust_type "veoveo_types::permission_names::PermissionName"]
abbrev veoveo_types.permission_names.PermissionName := CurrentRolesId 11

/-- [veoveo_types::resource_template::ResourceTemplateUri]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource_template.rs', lines 23:0-23:30
    Name pattern: [veoveo_types::resource_template::ResourceTemplateUri]
    Visibility: public -/
@[rust_type "veoveo_types::resource_template::ResourceTemplateUri"]
abbrev veoveo_types.resource_template.ResourceTemplateUri := CurrentRolesId 12

/-- [veoveo_types::resource::ResourceUri]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/resource.rs', lines 10:0-10:22
    Name pattern: [veoveo_types::resource::ResourceUri]
    Visibility: public -/
@[rust_type "veoveo_types::resource::ResourceUri"]
abbrev veoveo_types.resource.ResourceUri := CurrentRolesId 13

/- In-progress CurrentPermissions extension. Only the four iterator/Arc types
are operational in this slice. Other new payloads are uninterpreted nominal
values or immutable field containers: no HTTP/gateway/locking/JSON/registry
implementation, allocator behavior or serialization claim follows. Arc models
its referenced VALUE; native lifetime, alias and reference-count refinement is
explicitly outside the model. Earlier type models above remain unchanged. -/
universe uI uU uF uItem uInner

/-- [core::iter::adapters::flatten::FlatMap]
    Source: '/rustc/library/core/src/iter/adapters/flatten.rs', lines 17:0-17:41
    Name pattern: [core::iter::adapters::flatten::FlatMap]
    Visibility: public -/
@[rust_type "core::iter::adapters::flatten::FlatMap"]
structure core.iter.adapters.flatten.FlatMap
    (I : Type uI) (U : Type uU) (F : Type uF)
    (Item : Type uItem) (Inner : Type uInner) where
  iter : I
  mapper : F
  inner : Option Inner
  exhausted : Bool

/-- [core::net::ip_addr::Ipv6Addr]
    Source: '/rustc/library/core/src/net/ip_addr.rs', lines 168:0-168:19
    Name pattern: [core::net::ip_addr::Ipv6Addr]
    Visibility: public -/
@[rust_type "core::net::ip_addr::Ipv6Addr"]
abbrev core.net.ip_addr.Ipv6Addr := CurrentRolesId 14

/-- [core::net::ip_addr::Ipv4Addr]
    Source: '/rustc/library/core/src/net/ip_addr.rs', lines 74:0-74:19
    Name pattern: [core::net::ip_addr::Ipv4Addr]
    Visibility: public -/
@[rust_type "core::net::ip_addr::Ipv4Addr"]
abbrev core.net.ip_addr.Ipv4Addr := CurrentRolesId 15

/-- [core::num::niche_types::NonZeroUsizeInner]
    Source: '/rustc/library/core/src/num/niche_types.rs', lines 20:8-20:55; this code is generated from a macro invocation at: '/rustc/library/core/src/num/niche_types.rs', lines 115:0-139:1
    Name pattern: [core::num::niche_types::NonZeroUsizeInner]
    Visibility: public -/
@[rust_type "core::num::niche_types::NonZeroUsizeInner"]
abbrev core.num.niche_types.NonZeroUsizeInner := CurrentRolesId 16

/-- [core::num::niche_types::NonZeroU64Inner]
    Source: '/rustc/library/core/src/num/niche_types.rs', lines 20:8-20:55; this code is generated from a macro invocation at: '/rustc/library/core/src/num/niche_types.rs', lines 115:0-139:1
    Name pattern: [core::num::niche_types::NonZeroU64Inner]
    Visibility: public -/
@[rust_type "core::num::niche_types::NonZeroU64Inner"]
abbrev core.num.niche_types.NonZeroU64Inner := CurrentRolesId 17

/-- [core::num::nonzero::NonZero]
    Source: '/rustc/library/core/src/num/nonzero.rs', lines 112:0-112:40
    Name pattern: [core::num::nonzero::NonZero]
    Visibility: public -/
@[rust_type "core::num::nonzero::NonZero"]
abbrev core.num.nonzero.NonZero (T : Type _) (Inner : Type _) := T

/-- [std::collections::hash::map::HashMap]
    Source: '/rustc/library/std/src/collections/hash/map.rs', lines 245:0-250:1
    Name pattern: [std::collections::hash::map::HashMap]
    Visibility: public -/
@[rust_type "std::collections::hash::map::HashMap"]
abbrev std.collections.hash.map.HashMap (K : Type _) (V : Type _) (S : Type _) (A : Type _) := List (K × V)

/-- [std::hash::random::RandomState]
    Source: '/rustc/library/std/src/hash/random.rs', lines 35:0-35:22
    Name pattern: [std::hash::random::RandomState]
    Visibility: public -/
@[rust_type "std::hash::random::RandomState"]
abbrev std.hash.random.RandomState := CurrentRolesId 18

/-- [std::path::PathBuf]
    Source: '/rustc/library/std/src/path.rs', lines 1214:0-1214:18
    Name pattern: [std::path::PathBuf]
    Visibility: public -/
@[rust_type "std::path::PathBuf"]
abbrev std.path.PathBuf := CurrentRolesId 19

/-- [alloc::collections::btree::map::Values]
    Source: '/rustc/library/alloc/src/collections/btree/map.rs', lines 516:0-516:27
    Name pattern: [alloc::collections::btree::map::Values]
    Visibility: public -/
@[rust_type "alloc::collections::btree::map::Values"]
abbrev alloc.collections.btree.map.Values (K : Type _) (V : Type _) := List V

/-- [alloc::collections::btree::set::IntoIter]
    Source: '/rustc/library/alloc/src/collections/btree/set.rs', lines 154:0-157:1
    Name pattern: [alloc::collections::btree::set::IntoIter]
    Visibility: public -/
@[rust_type "alloc::collections::btree::set::IntoIter"]
abbrev alloc.collections.btree.set.IntoIter (T : Type _) (A : Type _) := List T

/-- [alloc::sync::Arc]
    Source: '/rustc/library/alloc/src/sync.rs', lines 274:0-277:1
    Name pattern: [alloc::sync::Arc]
    Visibility: public -/
@[rust_type "alloc::sync::Arc"]
abbrev alloc.sync.Arc (T : Type _) := T

/-- [openshell_sdk::client::OpenShellClient]
    Source: '/cargo/git/checkouts/openshell-f2a3eacab63ae477/c28524b/crates/openshell-sdk/src/client.rs', lines 37:0-37:26
    Name pattern: [openshell_sdk::client::OpenShellClient]
    Visibility: public -/
@[rust_type "openshell_sdk::client::OpenShellClient"]
abbrev openshell_sdk.client.OpenShellClient := CurrentRolesId 20

/-- [openshell_sdk::config::AuthConfig]
    Source: '/cargo/git/checkouts/openshell-f2a3eacab63ae477/c28524b/crates/openshell-sdk/src/config.rs', lines 26:0-26:19
    Name pattern: [openshell_sdk::config::AuthConfig]
    Visibility: public -/
@[rust_type "openshell_sdk::config::AuthConfig"]
abbrev openshell_sdk.config.AuthConfig := CurrentRolesId 21

/-- [reqwest#1::async_impl::client::Client]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reqwest-0.13.5/src/async_impl/client.rs', lines 93:0-93:17
    Name pattern: [reqwest#1::async_impl::client::Client]
    Visibility: public -/
@[rust_type "reqwest#1::async_impl::client::Client"]
abbrev reqwest_1.async_impl.client.Client := CurrentRolesId 22

/-- [serde_json::map::Map]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.151/src/map.rs', lines 29:0-29:20
    Name pattern: [serde_json::map::Map]
    Visibility: public -/
@[rust_type "serde_json::map::Map"]
abbrev serde_json.map.Map (K : Type _) (V : Type _) := List (K × V)

/-- [serde_json::number::Number]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.151/src/number.rs', lines 22:0-22:17
    Name pattern: [serde_json::number::Number]
    Visibility: public -/
@[rust_type "serde_json::number::Number"]
abbrev serde_json.number.Number := CurrentRolesId 23

/-- [tokio::sync::mutex::Mutex]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/sync/mutex.rs', lines 133:0-133:27
    Name pattern: [tokio::sync::mutex::Mutex]
    Visibility: public -/
@[rust_type "tokio::sync::mutex::Mutex"]
abbrev tokio.sync.mutex.Mutex (T : Type _) := T

/-- [tokio::sync::once_cell::OnceCell]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/sync/once_cell.rs', lines 70:0-70:22
    Name pattern: [tokio::sync::once_cell::OnceCell]
    Visibility: public -/
@[rust_type "tokio::sync::once_cell::OnceCell"]
abbrev tokio.sync.once_cell.OnceCell (T : Type _) := Option T

/-- [tokio::sync::semaphore::Semaphore]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/sync/semaphore.rs', lines 427:0-427:20
    Name pattern: [tokio::sync::semaphore::Semaphore]
    Visibility: public -/
@[rust_type "tokio::sync::semaphore::Semaphore"]
abbrev tokio.sync.semaphore.Semaphore := CurrentRolesId 24

/-- [tokio::time::instant::Instant]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/time/instant.rs', lines 34:0-34:18
    Name pattern: [tokio::time::instant::Instant]
    Visibility: public -/
@[rust_type "tokio::time::instant::Instant"]
abbrev tokio.time.instant.Instant := CurrentRolesId 25

/-- [url::Url]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/url-2.5.8/src/lib.rs', lines 228:0-228:14
    Name pattern: [url::Url]
    Visibility: public -/
@[rust_type "url::Url"]
abbrev url.Url := CurrentRolesId 26

/-- [veoveo_audit_contract::ids::AuditRequestId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/audit/contract/src/ids.rs', lines 17:0-17:25
    Name pattern: [veoveo_audit_contract::ids::AuditRequestId]
    Visibility: public -/
@[rust_type "veoveo_audit_contract::ids::AuditRequestId"]
abbrev veoveo_audit_contract.ids.AuditRequestId := CurrentRolesId 27

/-- [veoveo_audit_contract::ids::AuditTraceId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/audit/contract/src/ids.rs', lines 23:0-23:23
    Name pattern: [veoveo_audit_contract::ids::AuditTraceId]
    Visibility: public -/
@[rust_type "veoveo_audit_contract::ids::AuditTraceId"]
abbrev veoveo_audit_contract.ids.AuditTraceId := CurrentRolesId 28

/-- [veoveo_audit_contract::ids::AuditSpanId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/audit/contract/src/ids.rs', lines 26:0-26:22
    Name pattern: [veoveo_audit_contract::ids::AuditSpanId]
    Visibility: public -/
@[rust_type "veoveo_audit_contract::ids::AuditSpanId"]
abbrev veoveo_audit_contract.ids.AuditSpanId := CurrentRolesId 29

/-- [veoveo_types::task::TaskId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/task.rs', lines 17:0-17:17
    Name pattern: [veoveo_types::task::TaskId]
    Visibility: public -/
@[rust_type "veoveo_types::task::TaskId"]
abbrev veoveo_types.task.TaskId := CurrentRolesId 30

/-- [veoveo_hub_sdk::execution::CompositionRevision]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/execution.rs', lines 20:0-20:30
    Name pattern: [veoveo_hub_sdk::execution::CompositionRevision]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::execution::CompositionRevision"]
abbrev veoveo_hub_sdk.execution.CompositionRevision := CurrentRolesId 31

/-- [veoveo_types::platform_names::OAuthClientId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 40:0-40:24
    Name pattern: [veoveo_types::platform_names::OAuthClientId]
    Visibility: public -/
@[rust_type "veoveo_types::platform_names::OAuthClientId"]
abbrev veoveo_types.platform_names.OAuthClientId := CurrentRolesId 32

/-- [veoveo_types::identity::DelegationId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 22:0-22:23
    Name pattern: [veoveo_types::identity::DelegationId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::DelegationId"]
abbrev veoveo_types.identity.DelegationId := CurrentRolesId 33

/-- [veoveo_types::identity::WorkContextId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 18:0-18:24
    Name pattern: [veoveo_types::identity::WorkContextId]
    Visibility: public -/
@[rust_type "veoveo_types::identity::WorkContextId"]
abbrev veoveo_types.identity.WorkContextId := CurrentRolesId 34

/-- [veoveo_hub_sdk::identity::JwtId]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 168:0-168:16
    Name pattern: [veoveo_hub_sdk::identity::JwtId]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::JwtId"]
abbrev veoveo_hub_sdk.identity.JwtId := CurrentRolesId 35

/-- [veoveo_hub_sdk::identity::ProtectedResourceId]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 164:0-164:30
    Name pattern: [veoveo_hub_sdk::identity::ProtectedResourceId]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::ProtectedResourceId"]
abbrev veoveo_hub_sdk.identity.ProtectedResourceId := CurrentRolesId 36

/-- [veoveo_hub_sdk::identity::HubRefreshFamilyId]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 160:0-160:29
    Name pattern: [veoveo_hub_sdk::identity::HubRefreshFamilyId]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::HubRefreshFamilyId"]
abbrev veoveo_hub_sdk.identity.HubRefreshFamilyId := CurrentRolesId 37

/-- [veoveo_hub_sdk::identity::HubProfileId]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 155:0-155:23
    Name pattern: [veoveo_hub_sdk::identity::HubProfileId]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::HubProfileId"]
abbrev veoveo_hub_sdk.identity.HubProfileId := CurrentRolesId 38

/-- [veoveo_hub_sdk::identity::OpaqueTaskId]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/identity.rs', lines 184:0-184:23
    Name pattern: [veoveo_hub_sdk::identity::OpaqueTaskId]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::identity::OpaqueTaskId"]
abbrev veoveo_hub_sdk.identity.OpaqueTaskId := CurrentRolesId 39

/-- [veoveo_types::platform_names::ServerSlug]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/platform_names.rs', lines 6:0-6:21
    Name pattern: [veoveo_types::platform_names::ServerSlug]
    Visibility: public -/
@[rust_type "veoveo_types::platform_names::ServerSlug"]
abbrev veoveo_types.platform_names.ServerSlug := CurrentRolesId 40

/-- [veoveo_types::execution::DeploymentId]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/execution.rs', lines 19:0-19:23
    Name pattern: [veoveo_types::execution::DeploymentId]
    Visibility: public -/
@[rust_type "veoveo_types::execution::DeploymentId"]
abbrev veoveo_types.execution.DeploymentId := CurrentRolesId 41

/-- [veoveo_types::identity::PolicyVersion]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/identity.rs', lines 34:0-34:24
    Name pattern: [veoveo_types::identity::PolicyVersion]
    Visibility: public -/
@[rust_type "veoveo_types::identity::PolicyVersion"]
abbrev veoveo_types.identity.PolicyVersion := CurrentRolesId 42

/-- [veoveo_hub_sdk::registration::declaration::ServerVersion]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/declaration.rs', lines 26:0-26:24
    Name pattern: [veoveo_hub_sdk::registration::declaration::ServerVersion]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::registration::declaration::ServerVersion"]
abbrev veoveo_hub_sdk.registration.declaration.ServerVersion := CurrentRolesId 43

/-- [veoveo_hub_sdk::registration::declaration::ServerEndpoint]
    Source: '/cargo/git/checkouts/veoveo-hub-sdk-bce286867adf89df/05273d1/crates/veoveo-hub-sdk/src/registration/declaration.rs', lines 70:0-70:25
    Name pattern: [veoveo_hub_sdk::registration::declaration::ServerEndpoint]
    Visibility: public -/
@[rust_type "veoveo_hub_sdk::registration::declaration::ServerEndpoint"]
abbrev veoveo_hub_sdk.registration.declaration.ServerEndpoint := CurrentRolesId 44

/-- [veoveo_types::actions::ActionName]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/actions.rs', lines 7:0-7:21
    Name pattern: [veoveo_types::actions::ActionName]
    Visibility: public -/
@[rust_type "veoveo_types::actions::ActionName"]
abbrev veoveo_types.actions.ActionName := CurrentRolesId 45

/-- [veoveo_types::actions::ActionRegistry]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/actions.rs', lines 177:0-177:25
    Name pattern: [veoveo_types::actions::ActionRegistry]
    Visibility: public -/
@[rust_type "veoveo_types::actions::ActionRegistry"]
abbrev veoveo_types.actions.ActionRegistry := CurrentRolesId 46

/-- [veoveo_types::extensions::ExtensionName]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/extensions.rs', lines 16:0-16:24
    Name pattern: [veoveo_types::extensions::ExtensionName]
    Visibility: public -/
@[rust_type "veoveo_types::extensions::ExtensionName"]
abbrev veoveo_types.extensions.ExtensionName := CurrentRolesId 47

/-- [veoveo_types::extensions::ExtensionRegistry]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/extensions.rs', lines 140:0-140:28
    Name pattern: [veoveo_types::extensions::ExtensionRegistry]
    Visibility: public -/
@[rust_type "veoveo_types::extensions::ExtensionRegistry"]
abbrev veoveo_types.extensions.ExtensionRegistry := CurrentRolesId 48

/-- [veoveo_types::extensions::AdmittedExtensions]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/extensions.rs', lines 236:0-236:29
    Name pattern: [veoveo_types::extensions::AdmittedExtensions]
    Visibility: public -/
@[rust_type "veoveo_types::extensions::AdmittedExtensions"]
abbrev veoveo_types.extensions.AdmittedExtensions := CurrentRolesId 49

/-- [veoveo_types::names::ResourceScheme]
    Source: '/cargo/git/checkouts/veoveo-core-becc5d46deb07dab/120306e/platform/types/src/names.rs', lines 8:0-8:25
    Name pattern: [veoveo_types::names::ResourceScheme]
    Visibility: public -/
@[rust_type "veoveo_types::names::ResourceScheme"]
abbrev veoveo_types.names.ResourceScheme := CurrentRolesId 50

/-- [veoveo_hub::mcp::upstream_http::HubUpstreamHttpClientPool]
    Source: 'crates/veoveo-hub/src/mcp/upstream_http.rs', lines 25:0-30:1
    Visibility: public -/
abbrev mcp.upstream_http.HubUpstreamHttpClientPool := CurrentRolesId 51

/-- [veoveo_hub::contract::catalog::TargetDeclaration]
    Source: 'crates/veoveo-hub/src/contract/catalog.rs', lines 156:0-160:1 -/
abbrev contract.catalog.TargetDeclaration := CurrentRolesId 52

/-- [veoveo_hub::contract::catalog::SectionDeclaration]
    Source: 'crates/veoveo-hub/src/contract/catalog.rs', lines 151:0-154:1 -/
abbrev contract.catalog.SectionDeclaration := CurrentRolesId 53


/- Unused native metadata payloads. No JSON/protobuf parsing, floating-point
arithmetic, encoding or serialization behavior is claimed. These distinct nominal
values preserve the fact that arbitrary payloads may occur in captured state. -/
/-- [prost_types::protobuf::value::Kind]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/prost-types-0.14.4/src/protobuf.rs', lines 2146:4-2146:17
    Name pattern: [prost_types::protobuf::value::Kind]
    Visibility: public -/
@[rust_type "prost_types::protobuf::value::Kind"]
abbrev prost_types.protobuf.value.Kind := CurrentRolesId 54

/-- [serde_json::value::Value]
    Source: '/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.151/src/value/mod.rs', lines 116:0-116:14
    Name pattern: [serde_json::value::Value]
    Visibility: public -/
@[rust_type "serde_json::value::Value"]
abbrev serde_json.value.Value := CurrentRolesId 55

/- Conditional native-policy extension. Nominal values remain unbounded;
ActionHandle retains a separate registry-identity token and its observed name.
This models value preservation, not native Arc pointer identity or admission. -/
@[rust_type "veoveo_hub_sdk::identity::CanonicalTaskId"]
abbrev veoveo_hub_sdk.identity.CanonicalTaskId := CurrentRolesId 56

@[rust_type "veoveo_types::extensions::ExtensionError"]
abbrev veoveo_types.extensions.ExtensionError := CurrentRolesId 57

@[rust_type "veoveo_types::actions::ActionHandle"]
structure veoveo_types.actions.ActionHandle where
  identity : CurrentRolesId 58
  actionName : veoveo_types.actions.ActionName

/- SDK access extension: map enumeration retains every raw entry. The native
parser's value is a distinct, unbounded nominal observation; token identity is
not a claim about Rust borrows, allocation or parser grammar. -/
@[rust_type "alloc::collections::btree::map::Iter"]
abbrev alloc.collections.btree.map.Iter (K : Type _) (V : Type _) := List (K × V)

@[rust_type
  "veoveo_hub_sdk::registration::authorization_template::AuthorizationTemplate"]
structure veoveo_hub_sdk.registration.authorization_template.AuthorizationTemplate where
  token : Nat

-- Session-family UTC dictionary payloads. These values are not inspected by
-- admission; native calendar/formatting/offset refinement is outside this slice.
@[rust_type "chrono::offset::fixed::FixedOffset"]
structure chrono.offset.fixed.FixedOffset where
  seconds : Int

@[rust_type "chrono::naive::date::NaiveDate"]
structure chrono.naive.date.NaiveDate where
  days : Int

@[rust_type "chrono::naive::datetime::NaiveDateTime"]
structure chrono.naive.datetime.NaiveDateTime where
  seconds : Int
  nanos : Nat
