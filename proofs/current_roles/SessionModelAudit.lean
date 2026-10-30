import HubCurrentRoles
import Lean.Util.CollectAxioms

open Aeneas Aeneas.Std veoveo_hub

/- Dependency audit of the actual unchanged session method and its ordinary
primitive models; no sampled clock or admission/result axiom is introduced. -/
#print axioms policy.session.SessionFamilyAuthority.allows
#print axioms alloc.string.String.Insts.CoreCmpPartialEqString.eq
#print axioms chrono.datetime.DateTime.Insts.CoreCmpPartialEqDateTime.eq
#print axioms chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp
#print axioms chrono.offset.utc.Utc.Insts.CoreCloneClone.clone
#print axioms chrono.offset.utc.Utc.Insts.CoreFmtDebug.fmt
#print axioms chrono.offset.utc.Utc.Insts.ChronoOffsetOffset.fix
#print axioms chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_utc_datetime
#print axioms chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_utc_date
#print axioms chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_local_datetime
#print axioms chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_local_date
#print axioms chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.from_offset
#print axioms veoveo_hub_sdk.identity.PrincipalKind.Insts.CoreCmpPartialEqPrincipalKind.eq
#print axioms veoveo_hub_sdk.identity.ProtectedResourceId.Insts.CoreCmpPartialEqProtectedResourceId.eq
#print axioms veoveo_types.identity.WorkContextId.Insts.CoreCmpPartialEqWorkContextId.eq
#print axioms veoveo_types.identity.TokenIssuer.Insts.CoreCmpPartialEqTokenIssuer.eq
#print axioms veoveo_types.identity.TokenSubject.Insts.CoreCmpPartialEqTokenSubject.eq
#print axioms veoveo_types.names.ScopeName.Insts.CoreCmpOrd.cmp
#print axioms veoveo_types.names.ScopeName.Insts.CoreCmpPartialOrdScopeName.partial_cmp
#print axioms veoveo_types.names.ScopeName.Insts.CoreCmpPartialEqScopeName.eq
#print axioms veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpPartialEqOAuthClientId.eq

run_cmd do
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  for declaration in #[``policy.session.SessionFamilyAuthority.allows,
      ``alloc.string.String.Insts.CoreCmpPartialEqString.eq,
      ``chrono.datetime.DateTime.Insts.CoreCmpPartialEqDateTime.eq,
      ``chrono.datetime.DateTime.Insts.CoreCmpPartialOrdDateTime.partial_cmp,
      ``chrono.offset.utc.Utc.Insts.CoreCloneClone.clone,
      ``chrono.offset.utc.Utc.Insts.CoreFmtDebug.fmt,
      ``chrono.offset.utc.Utc.Insts.ChronoOffsetOffset.fix,
      ``chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_utc_datetime,
      ``chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_utc_date,
      ``chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_local_datetime,
      ``chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.offset_from_local_date,
      ``chrono.offset.utc.Utc.Insts.ChronoOffsetTimeZoneUtc.from_offset,
      ``veoveo_hub_sdk.identity.PrincipalKind.Insts.CoreCmpPartialEqPrincipalKind.eq,
      ``veoveo_hub_sdk.identity.ProtectedResourceId.Insts.CoreCmpPartialEqProtectedResourceId.eq,
      ``veoveo_types.identity.WorkContextId.Insts.CoreCmpPartialEqWorkContextId.eq,
      ``veoveo_types.identity.TokenIssuer.Insts.CoreCmpPartialEqTokenIssuer.eq,
      ``veoveo_types.identity.TokenSubject.Insts.CoreCmpPartialEqTokenSubject.eq,
      ``veoveo_types.names.ScopeName.Insts.CoreCmpOrd.cmp,
      ``veoveo_types.names.ScopeName.Insts.CoreCmpPartialOrdScopeName.partial_cmp,
      ``veoveo_types.names.ScopeName.Insts.CoreCmpPartialEqScopeName.eq,
      ``veoveo_types.platform_names.OAuthClientId.Insts.CoreCmpPartialEqOAuthClientId.eq] do
    let found ← Lean.collectAxioms declaration
    unless (found.filter (fun axiomName => !allowed.contains axiomName)).isEmpty do
      throwError "unreviewed session model axiom in {declaration}: {found}"
