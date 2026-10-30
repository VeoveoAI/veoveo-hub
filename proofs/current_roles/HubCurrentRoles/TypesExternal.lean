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