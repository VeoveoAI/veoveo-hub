//! Role evaluation over registered servers' declared permissions. Pure: no I/O, no
//! clock, no server names. Installation roles select permissions by access kind and
//! risk; grants give roles to groups and principals within a tenant.
use std::collections::BTreeSet;

use crate::control_plane::{GrantSubject, InstallationAuthorization, PermissionOverride};
use veoveo_hub_sdk::Principal;
use veoveo_hub_sdk::registration::{AuthorizationDeclaration, PermissionDeclaration, Risk};
use veoveo_types::{PermissionName, RoleId};

/// The installation roles `principal` holds: grants to the principal itself or to one of
/// its groups, in its tenant or in every tenant.
pub fn held_roles(
    installation: &InstallationAuthorization,
    principal: &Principal,
) -> BTreeSet<RoleId> {
    installation
        .grants
        .iter()
        .filter(|grant| {
            grant
                .tenant
                .as_ref()
                .is_none_or(|tenant| principal.tenant.as_ref() == Some(tenant))
        })
        .filter(|grant| match &grant.subject {
            GrantSubject::Group(group) => principal.groups.contains(group),
            GrantSubject::Principal(id) => &principal.id == id,
        })
        .map(|grant| grant.role.clone())
        .collect()
}

/// Every permission of one server's declaration that `principal` holds. A permission
/// is granted when some held role allows its access kind and every one of its risks,
/// after installation overrides; a denied permission is never granted.
pub fn granted_permissions(
    installation: &InstallationAuthorization,
    declaration: &AuthorizationDeclaration,
    principal: &Principal,
) -> BTreeSet<PermissionName> {
    let held = held_roles(installation, principal);
    let roles: Vec<_> = installation
        .roles
        .iter()
        .filter(|role| held.contains(&role.name))
        .collect();
    declaration
        .permissions
        .iter()
        .filter(|permission| {
            let Some(risks) = effective_risks(installation, permission) else {
                return false;
            };
            roles
                .iter()
                .any(|role| role.allows(permission.access, &risks))
        })
        .map(|permission| permission.permission.clone())
        .collect()
}

/// Declared risks plus installation additions; `None` when the installation denies it.
fn effective_risks(
    installation: &InstallationAuthorization,
    permission: &PermissionDeclaration,
) -> Option<BTreeSet<Risk>> {
    let mut risks = permission.risks.clone();
    for permission_override in &installation.overrides {
        if permission_override.permission() != &permission.permission {
            continue;
        }
        match permission_override {
            PermissionOverride::Deny { .. } => return None,
            PermissionOverride::AddRisks { risks: added, .. } => risks.extend(added),
        }
    }
    Some(risks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::{InstallationRole, RoleGrant};
    use veoveo_hub_sdk::PrincipalKind;
    use veoveo_hub_sdk::registration::{Access, PermissionDeclaration, Requirements};
    use veoveo_types::{GroupId, PrincipalId, ResourceTypeName, TenantId};

    fn permission(name: &str, access: Access, risks: &[Risk]) -> PermissionDeclaration {
        PermissionDeclaration {
            permission: PermissionName::parse(name).unwrap(),
            resource: ResourceTypeName::parse("time.authority").unwrap(),
            access,
            risks: risks.iter().copied().collect(),
        }
    }

    fn declaration() -> AuthorizationDeclaration {
        AuthorizationDeclaration {
            permissions: vec![
                permission("time.read", Access::Read, &[]),
                permission("time.events.write", Access::Write, &[]),
                permission("time.admin", Access::Write, &[Risk::Administer]),
                permission("time.exports.read", Access::Read, &[Risk::Sensitive]),
            ],
            requirements: Requirements::default(),
            uses: Vec::new(),
            calls_hub: false,
        }
    }

    fn role(name: &str, access: &[Access], risks: &[Risk]) -> InstallationRole {
        InstallationRole {
            name: RoleId::parse(name).unwrap(),
            access: access.iter().copied().collect(),
            risks: risks.iter().copied().collect(),
        }
    }

    fn grant(role: &str, group: &str, tenant: Option<&str>) -> RoleGrant {
        RoleGrant {
            role: RoleId::parse(role).unwrap(),
            subject: GrantSubject::Group(GroupId::parse(group).unwrap()),
            tenant: tenant.map(|tenant| TenantId::parse(tenant).unwrap()),
        }
    }

    fn installation() -> InstallationAuthorization {
        InstallationAuthorization {
            roles: vec![
                role("member", &[Access::Read, Access::Write], &[]),
                role(
                    "admin",
                    &[Access::Read, Access::Write],
                    &[Risk::Sensitive, Risk::Administer],
                ),
            ],
            grants: vec![
                grant("member", "tenant-members", Some("dev")),
                grant("admin", "installation-admins", None),
            ],
            overrides: Vec::new(),
        }
    }

    fn principal(groups: &[&str], tenant: &str) -> Principal {
        Principal {
            id: PrincipalId::parse("user:alice").unwrap(),
            kind: PrincipalKind::User,
            issuer: veoveo_types::TokenIssuer::parse("https://idp.example").unwrap(),
            subject: veoveo_types::TokenSubject::parse("alice").unwrap(),
            tenant: Some(TenantId::parse(tenant).unwrap()),
            groups: groups.iter().map(|g| GroupId::parse(*g).unwrap()).collect(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        }
    }

    fn names(set: BTreeSet<PermissionName>) -> Vec<String> {
        set.into_iter()
            .map(|name| name.as_str().to_owned())
            .collect()
    }

    #[test]
    fn a_role_grants_only_permissions_whose_risks_it_allows() {
        let member = principal(&["tenant-members"], "dev");
        assert_eq!(
            names(granted_permissions(
                &installation(),
                &declaration(),
                &member
            )),
            ["time.events.write", "time.read"]
        );
        let admin = principal(&["installation-admins"], "other");
        assert_eq!(
            names(granted_permissions(&installation(), &declaration(), &admin)),
            [
                "time.admin",
                "time.events.write",
                "time.exports.read",
                "time.read"
            ]
        );
    }

    #[test]
    fn grants_are_scoped_to_their_tenant() {
        let member_elsewhere = principal(&["tenant-members"], "other");
        assert!(granted_permissions(&installation(), &declaration(), &member_elsewhere).is_empty());
        let nobody = principal(&[], "dev");
        assert!(granted_permissions(&installation(), &declaration(), &nobody).is_empty());
    }

    #[test]
    fn a_risk_no_role_names_fails_closed() {
        let mut declaration = declaration();
        declaration.permissions.push(permission(
            "time.drones.launch",
            Access::Write,
            &[Risk::Physical],
        ));
        let admin = principal(&["installation-admins"], "dev");
        assert!(
            !granted_permissions(&installation(), &declaration, &admin)
                .contains(&PermissionName::parse("time.drones.launch").unwrap())
        );
    }

    #[test]
    fn overrides_deny_or_add_risks() {
        let member = principal(&["tenant-members"], "dev");
        let mut installation = installation();
        installation.overrides = vec![
            PermissionOverride::Deny {
                permission: PermissionName::parse("time.read").unwrap(),
            },
            PermissionOverride::AddRisks {
                permission: PermissionName::parse("time.events.write").unwrap(),
                risks: BTreeSet::from([Risk::Destructive]),
            },
        ];
        assert!(granted_permissions(&installation, &declaration(), &member).is_empty());
        let admin = principal(&["installation-admins"], "dev");
        let granted = names(granted_permissions(&installation, &declaration(), &admin));
        assert!(!granted.contains(&"time.read".to_owned()));
        // Admins lack the destructive risk, so the added risk removes the grant too.
        assert!(!granted.contains(&"time.events.write".to_owned()));
        assert!(granted.contains(&"time.admin".to_owned()));
    }

    #[test]
    fn grants_to_one_principal_apply_only_to_it() {
        let mut installation = installation();
        installation.grants = vec![RoleGrant {
            role: RoleId::parse("member").unwrap(),
            subject: GrantSubject::Principal(PrincipalId::parse("user:alice").unwrap()),
            tenant: None,
        }];
        let alice = principal(&[], "dev");
        assert_eq!(
            names(granted_permissions(&installation, &declaration(), &alice)),
            ["time.events.write", "time.read"]
        );
        let mut bob = principal(&[], "dev");
        bob.id = PrincipalId::parse("user:bob").unwrap();
        assert!(granted_permissions(&installation, &declaration(), &bob).is_empty());
    }
    fn fixture_permission(access: Access, risks: &[Risk]) -> PermissionDeclaration {
        PermissionDeclaration {
            permission: PermissionName::parse("fixture.inspect").unwrap(),
            resource: ResourceTypeName::parse("fixture.item").unwrap(),
            access,
            risks: risks.iter().copied().collect(),
        }
    }

    fn fixture_declaration(permissions: Vec<PermissionDeclaration>) -> AuthorizationDeclaration {
        AuthorizationDeclaration {
            permissions,
            requirements: Requirements::default(),
            uses: Vec::new(),
            calls_hub: false,
        }
    }

    fn fixture_installation() -> InstallationAuthorization {
        InstallationAuthorization {
            roles: vec![role("fixture-member", &[Access::Read], &[])],
            grants: vec![grant("fixture-member", "fixture-group", None)],
            overrides: Vec::new(),
        }
    }

    fn fixture_addition(permission: &str, risks: &[Risk]) -> PermissionOverride {
        PermissionOverride::AddRisks {
            permission: PermissionName::parse(permission).unwrap(),
            risks: risks.iter().copied().collect(),
        }
    }

    #[test]
    fn tenant_scope_preserves_missing_and_matching_tenants() {
        let declaration = fixture_declaration(vec![fixture_permission(Access::Read, &[])]);
        for (grant_tenant, caller_tenant, expected) in [
            (Some("dev"), Some("dev"), true),
            (Some("dev"), None, false),
            (Some("dev"), Some("other"), false),
            (None, None, true),
            (None, Some("dev"), true),
            (None, Some("other"), true),
        ] {
            let mut installation = fixture_installation();
            installation.grants[0].tenant = grant_tenant.map(|t| TenantId::parse(t).unwrap());
            let mut caller = principal(&["fixture-group"], "dev");
            caller.tenant = caller_tenant.map(|t| TenantId::parse(t).unwrap());
            let expected_roles = if expected {
                BTreeSet::from([RoleId::parse("fixture-member").unwrap()])
            } else {
                BTreeSet::new()
            };
            assert_eq!(
                held_roles(&installation, &caller),
                expected_roles,
                "grant tenant {grant_tenant:?}, caller tenant {caller_tenant:?}"
            );
            let expected_permissions = if expected {
                vec!["fixture.inspect".to_owned()]
            } else {
                Vec::new()
            };
            assert_eq!(
                names(granted_permissions(&installation, &declaration, &caller)),
                expected_permissions
            );
        }
    }

    #[test]
    fn subjects_match_only_direct_id_or_group_membership() {
        let group = GroupId::parse("fixture-group").unwrap();
        let declaration = fixture_declaration(vec![fixture_permission(Access::Read, &[])]);
        for (subject, groups, expected) in [
            (
                GrantSubject::Group(group.clone()),
                vec!["fixture-group"],
                true,
            ),
            (GrantSubject::Group(group.clone()), Vec::new(), false),
            (GrantSubject::Group(group), vec!["other-group"], false),
            (
                GrantSubject::Principal(PrincipalId::parse("user:alice").unwrap()),
                Vec::new(),
                true,
            ),
            (
                GrantSubject::Principal(PrincipalId::parse("user:bob").unwrap()),
                Vec::new(),
                false,
            ),
        ] {
            let mut installation = fixture_installation();
            installation.grants[0].subject = subject;
            let caller = principal(&groups, "dev");
            let expected_roles = if expected {
                BTreeSet::from([RoleId::parse("fixture-member").unwrap()])
            } else {
                BTreeSet::new()
            };
            assert_eq!(held_roles(&installation, &caller), expected_roles);
            let expected_permissions = if expected {
                vec!["fixture.inspect".to_owned()]
            } else {
                Vec::new()
            };
            assert_eq!(
                names(granted_permissions(&installation, &declaration, &caller)),
                expected_permissions
            );
        }
    }

    #[test]
    fn unheld_or_unconfigured_roles_do_not_grant() {
        let declaration = fixture_declaration(vec![fixture_permission(Access::Read, &[])]);
        let mut installation = fixture_installation();
        let mut caller = principal(&[], "dev");
        caller
            .roles
            .insert(RoleId::parse("fixture-member").unwrap());
        assert_eq!(
            names(granted_permissions(&installation, &declaration, &caller)),
            Vec::<String>::new()
        );
        caller
            .groups
            .insert(GroupId::parse("fixture-group").unwrap());
        installation.grants[0].role = RoleId::parse("missing-role").unwrap();
        assert_eq!(
            names(granted_permissions(&installation, &declaration, &caller)),
            Vec::<String>::new()
        );
        installation.grants[0].role = RoleId::parse("fixture-member").unwrap();
        assert_eq!(
            names(granted_permissions(&installation, &declaration, &caller)),
            ["fixture.inspect"]
        );
    }

    #[test]
    fn one_configured_role_must_cover_access_and_all_risks() {
        let permission = fixture_permission(Access::Read, &[Risk::Sensitive]);
        let declaration = fixture_declaration(vec![permission.clone()]);
        let caller = principal(&["fixture-group"], "dev");
        for second_name in ["risk-member", "fixture-member"] {
            let mut installation = fixture_installation();
            installation.roles = vec![
                role("fixture-member", &[Access::Read], &[]),
                role(second_name, &[Access::Write], &[Risk::Sensitive]),
            ];
            installation
                .grants
                .push(grant("risk-member", "fixture-group", None));
            assert_eq!(
                names(granted_permissions(&installation, &declaration, &caller)),
                Vec::<String>::new()
            );
            for configured in &installation.roles {
                assert!(
                    !configured.allows(permission.access, &permission.risks),
                    "{configured:?}"
                );
            }
            installation
                .roles
                .push(role("complete-member", &[Access::Read], &[Risk::Sensitive]));
            installation
                .grants
                .push(grant("complete-member", "fixture-group", None));
            assert_eq!(
                names(granted_permissions(&installation, &declaration, &caller)),
                ["fixture.inspect"]
            );
        }
    }

    #[test]
    fn overrides_accumulate_all_matching_risks() {
        let permission = fixture_permission(Access::Read, &[Risk::Sensitive]);
        let declaration = fixture_declaration(vec![permission.clone()]);
        let caller = principal(&["fixture-group"], "dev");
        let mut installation = fixture_installation();
        installation.overrides = vec![
            fixture_addition("fixture.other", &[Risk::Administer]),
            fixture_addition("fixture.inspect", &[Risk::Physical]),
            fixture_addition("fixture.inspect", &[Risk::Billed]),
            fixture_addition("fixture.inspect", &[Risk::Physical]),
            PermissionOverride::Deny {
                permission: PermissionName::parse("fixture.other").unwrap(),
            },
        ];
        assert_eq!(
            effective_risks(&installation, &permission),
            Some(BTreeSet::from([
                Risk::Sensitive,
                Risk::Physical,
                Risk::Billed
            ]))
        );
        installation.roles[0].risks = BTreeSet::from([Risk::Sensitive, Risk::Physical]);
        assert_eq!(
            names(granted_permissions(&installation, &declaration, &caller)),
            Vec::<String>::new()
        );
        installation.roles[0].risks.insert(Risk::Billed);
        assert_eq!(
            names(granted_permissions(&installation, &declaration, &caller)),
            ["fixture.inspect"]
        );
    }

    #[test]
    fn matching_deny_wins_in_any_position() {
        let permission = fixture_permission(Access::Read, &[Risk::Sensitive]);
        let declaration = fixture_declaration(vec![permission.clone()]);
        let caller = principal(&["fixture-group"], "dev");
        let addition = fixture_addition("fixture.inspect", &[Risk::Physical]);
        let other = fixture_addition("fixture.other", &[Risk::Administer]);
        let deny = PermissionOverride::Deny {
            permission: permission.permission.clone(),
        };
        for overrides in [
            vec![deny.clone(), addition.clone(), other.clone()],
            vec![addition.clone(), deny.clone(), other.clone()],
            vec![addition, other, deny],
        ] {
            let mut installation = fixture_installation();
            installation.roles[0].risks = BTreeSet::from([Risk::Sensitive, Risk::Physical]);
            installation.overrides = overrides;
            assert_eq!(effective_risks(&installation, &permission), None);
            assert_eq!(
                names(granted_permissions(&installation, &declaration, &caller)),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn valid_permission_occurrences_and_empty_inputs() {
        let installation = fixture_installation();
        let caller = principal(&["fixture-group"], "dev");
        let declaration = fixture_declaration(vec![fixture_permission(Access::Read, &[])]);
        assert_eq!(
            names(granted_permissions(&installation, &declaration, &caller)),
            ["fixture.inspect"]
        );
        let empty_declaration = fixture_declaration(Vec::new());
        assert_eq!(
            names(granted_permissions(
                &installation,
                &empty_declaration,
                &caller
            )),
            Vec::<String>::new()
        );
        let mut no_roles = installation.clone();
        no_roles.roles.clear();
        assert_eq!(
            names(granted_permissions(&no_roles, &declaration, &caller)),
            Vec::<String>::new()
        );
        let mut no_grants = installation.clone();
        no_grants.grants.clear();
        assert_eq!(
            names(granted_permissions(&no_grants, &declaration, &caller)),
            Vec::<String>::new()
        );
        let mut duplicates = fixture_declaration(vec![
            fixture_permission(Access::Read, &[Risk::Sensitive]),
            fixture_permission(Access::Write, &[]),
            fixture_permission(Access::Read, &[]),
        ]);
        assert_eq!(
            names(granted_permissions(&installation, &duplicates, &caller)),
            ["fixture.inspect"]
        );
        duplicates.permissions.pop();
        assert_eq!(
            names(granted_permissions(&installation, &duplicates, &caller)),
            Vec::<String>::new()
        );
    }
}
