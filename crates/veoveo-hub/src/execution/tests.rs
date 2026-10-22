use super::*;
use veoveo_hub_sdk::DeploymentId;

fn owner() -> EnvironmentOwner {
    EnvironmentOwner {
        tenant: TenantId::parse("tenant").unwrap(),
        principal: PrincipalId::parse("author").unwrap(),
    }
}

fn record(
    base: Option<EnvironmentBase>,
    components: &[(&str, ResolvedComponent)],
) -> EnvironmentRecord {
    EnvironmentRecord {
        owner: owner(),
        display_name: "Label only".to_owned(),
        readers: BTreeSet::new(),
        base,
        components: components
            .iter()
            .map(|(slug, value)| (ServerSlug::parse(slug).unwrap(), value.clone()))
            .collect(),
    }
}

fn selected(deployment: DeploymentId) -> ResolvedComponent {
    ResolvedComponent::Deployment { deployment }
}

#[test]
fn independent_overrides_inherit_without_taking_each_others_routes() {
    let base = EnvironmentId::new();
    let first = EnvironmentId::new();
    let second = EnvironmentId::new();
    let a = DeploymentId::new();
    let b = DeploymentId::new();
    let c = DeploymentId::new();
    let mut config = ExecutionConfiguration {
        default_environment: base,
        environments: BTreeMap::from([
            (base, record(None, &[("observatory", selected(a))])),
            (
                first,
                record(
                    Some(EnvironmentBase::Follow { environment: base }),
                    &[("observatory", selected(b))],
                ),
            ),
            (
                second,
                record(
                    Some(EnvironmentBase::Follow { environment: base }),
                    &[("observatory", selected(c))],
                ),
            ),
        ]),
        base_branch: None,
    };
    let resolve = |config: &ExecutionConfiguration, id| {
        config
            .resolve(id, &owner().tenant, &owner().principal, &BTreeMap::new())
            .unwrap()
    };
    let base_before = resolve(&config, base);
    let second_before = resolve(&config, second);
    config
        .environments
        .get_mut(&first)
        .unwrap()
        .components
        .insert(
            ServerSlug::parse("observatory").unwrap(),
            ResolvedComponent::Unavailable {},
        );
    assert_eq!(resolve(&config, base), base_before);
    assert_eq!(resolve(&config, second), second_before);
    assert_eq!(
        resolve(&config, first).components()[&ServerSlug::parse("observatory").unwrap()],
        ResolvedComponent::Unavailable {}
    );
    config
        .environments
        .get_mut(&first)
        .unwrap()
        .components
        .clear();
    assert_eq!(
        resolve(&config, first).components(),
        base_before.components()
    );
    config.environments.get_mut(&first).unwrap().base = None;
    assert!(resolve(&config, first).components().is_empty());
}

#[test]
fn pins_retain_exact_selection_while_followers_advance_and_labels_do_not_hash() {
    let base = EnvironmentId::new();
    let follower = EnvironmentId::new();
    let pin = EnvironmentId::new();
    let mut config = ExecutionConfiguration {
        default_environment: base,
        environments: BTreeMap::from([
            (
                base,
                record(None, &[("observatory", selected(DeploymentId::new()))]),
            ),
            (
                follower,
                record(Some(EnvironmentBase::Follow { environment: base }), &[]),
            ),
        ]),
        base_branch: None,
    };
    let original = config
        .resolve(base, &owner().tenant, &owner().principal, &BTreeMap::new())
        .unwrap();
    let pins = BTreeMap::from([(original.revision().clone(), original.clone())]);
    config.environments.insert(
        pin,
        record(
            Some(EnvironmentBase::Pin {
                environment: base,
                revision: original.revision().clone(),
            }),
            &[],
        ),
    );
    config
        .environments
        .get_mut(&base)
        .unwrap()
        .components
        .insert(
            ServerSlug::parse("observatory").unwrap(),
            selected(DeploymentId::new()),
        );
    let pinned = config
        .resolve(pin, &owner().tenant, &owner().principal, &pins)
        .unwrap();
    let followed = config
        .resolve(follower, &owner().tenant, &owner().principal, &pins)
        .unwrap();
    assert_eq!(pinned.components(), original.components());
    assert_ne!(followed.components(), pinned.components());
    config.environments.get_mut(&pin).unwrap().display_name = "Renamed".to_owned();
    assert_eq!(
        config
            .resolve(pin, &owner().tenant, &owner().principal, &pins)
            .unwrap(),
        pinned
    );
}

#[test]
fn cycles_cross_tenant_access_and_revoked_base_access_are_rejected() {
    let base = EnvironmentId::new();
    let child = EnvironmentId::new();
    let mut config = ExecutionConfiguration {
        default_environment: base,
        environments: BTreeMap::from([
            (base, record(None, &[])),
            (
                child,
                record(Some(EnvironmentBase::Follow { environment: base }), &[]),
            ),
        ]),
        base_branch: None,
    };
    config.environments.get_mut(&base).unwrap().base =
        Some(EnvironmentBase::Follow { environment: child });
    assert_eq!(config.validate(), Err(ExecutionError::Cycle));
    config.environments.get_mut(&base).unwrap().base = None;
    assert_eq!(
        config.resolve(
            child,
            &TenantId::parse("other").unwrap(),
            &owner().principal,
            &BTreeMap::new()
        ),
        Err(ExecutionError::Forbidden)
    );
    config.environments.get_mut(&base).unwrap().owner.principal =
        PrincipalId::parse("base-owner").unwrap();
    assert!(config.validate().is_ok());
    assert_eq!(
        config.resolve(child, &owner().tenant, &owner().principal, &BTreeMap::new()),
        Err(ExecutionError::Forbidden)
    );
    config
        .environments
        .get_mut(&base)
        .unwrap()
        .readers
        .insert(owner().principal);
    assert!(config.validate().is_ok());
    assert_eq!(
        config.resolve(
            child,
            &owner().tenant,
            &PrincipalId::parse("stranger").unwrap(),
            &BTreeMap::new()
        ),
        Err(ExecutionError::Forbidden)
    );
}

#[test]
fn retained_capture_checks_selected_access_without_reinterpreting_current_base() {
    let base = EnvironmentId::new();
    let child = EnvironmentId::new();
    let mut config = ExecutionConfiguration {
        default_environment: base,
        environments: BTreeMap::from([
            (
                base,
                record(None, &[("observatory", selected(DeploymentId::new()))]),
            ),
            (
                child,
                record(Some(EnvironmentBase::Follow { environment: base }), &[]),
            ),
        ]),
        base_branch: None,
    };
    let captured = config
        .resolve(child, &owner().tenant, &owner().principal, &BTreeMap::new())
        .unwrap();
    config.environments.get_mut(&base).unwrap().owner.principal =
        PrincipalId::parse("different-owner").unwrap();
    assert_eq!(
        config.resolve(child, &owner().tenant, &owner().principal, &BTreeMap::new()),
        Err(ExecutionError::Forbidden)
    );
    assert_eq!(
        config.authorize_capture(
            captured.context().environment,
            &owner().tenant,
            &owner().principal
        ),
        Ok(())
    );
    config.environments.get_mut(&child).unwrap().owner.principal =
        PrincipalId::parse("different-owner").unwrap();
    assert_eq!(
        config.authorize_capture(child, &owner().tenant, &owner().principal),
        Err(ExecutionError::Forbidden)
    );
}

fn branch(name: &str) -> BranchName {
    BranchName::parse(name).unwrap()
}

fn slug(name: &str) -> ServerSlug {
    ServerSlug::parse(name).unwrap()
}

#[test]
fn branch_names_follow_lore_limits() {
    assert!(BranchName::parse("dev/jose/x").is_ok());
    assert!(BranchName::parse("x".repeat(1000)).is_ok());
    assert!(BranchName::parse("x".repeat(1001)).is_err());
    assert!(BranchName::parse("").is_err());
    assert!(BranchName::parse("dev\nx").is_err());
}

#[test]
fn a_selection_routes_its_branch_and_never_falls_back() {
    use crate::registration::RegistrationSnapshot;
    use selection::select_instances;

    let instance = |slug: &str, name: &str| {
        let mut server = crate::test_catalog::registered_server(slug, "https://instance.test/mcp");
        server.branch = Some(branch(name));
        server
    };
    let dev = instance("observatory", "dev");
    let feature = instance("observatory", "dev/jose/x");
    let (dev_id, feature_id) = (dev.deployment, feature.deployment);
    let configured = ResolvedComposition::new(
        EnvironmentId::new(),
        BTreeMap::from([
            (slug("observatory"), selected(DeploymentId::new())),
            (slug("relay"), selected(DeploymentId::new())),
        ]),
    );
    let registrations = RegistrationSnapshot::new([dev, feature]);
    let base = branch("dev");
    let resolve = |base: Option<&BranchName>, selection: &[(&str, &str)]| {
        let selection = selection
            .iter()
            .map(|(server, name)| (slug(server), branch(name)))
            .collect();
        select_instances(configured.clone(), &registrations, base, &selection)
    };

    let unselected = resolve(Some(&base), &[]);
    assert_eq!(
        unselected.composition.components()[&slug("observatory")],
        selected(dev_id)
    );
    assert_eq!(
        unselected.composition.components()[&slug("relay")],
        configured.components()[&slug("relay")],
        "a server without instances keeps its configured component"
    );
    assert!(unselected.unavailable.is_empty());

    let chosen = resolve(Some(&base), &[("observatory", "dev/jose/x")]);
    assert_eq!(
        chosen.composition.components()[&slug("observatory")],
        selected(feature_id)
    );
    assert_ne!(
        chosen.composition.revision(),
        unselected.composition.revision()
    );

    let missing = resolve(
        Some(&base),
        &[("observatory", "dev/maria/y"), ("relay", "dev")],
    );
    for server in ["observatory", "relay"] {
        assert_eq!(
            missing.composition.components()[&slug(server)],
            ResolvedComponent::Unavailable {}
        );
    }
    assert_eq!(
        missing.unavailable[&slug("observatory")],
        Unavailable::Selected(branch("dev/maria/y"))
    );
    assert_eq!(
        missing.unavailable[&slug("relay")],
        Unavailable::Selected(branch("dev"))
    );

    let without_base = resolve(None, &[]);
    assert_eq!(
        without_base.unavailable[&slug("observatory")],
        Unavailable::NoBaseBranch
    );
    assert_eq!(
        resolve(None, &[("observatory", "dev")])
            .composition
            .components()[&slug("observatory")],
        selected(dev_id)
    );
    assert_eq!(
        resolve(Some(&branch("main")), &[]).unavailable[&slug("observatory")],
        Unavailable::Base(branch("main"))
    );

    let twice = RegistrationSnapshot::new([
        instance("observatory", "dev"),
        instance("observatory", "dev"),
    ]);
    let ambiguous = select_instances(configured.clone(), &twice, Some(&base), &BTreeMap::new());
    assert_eq!(
        ambiguous.unavailable[&slug("observatory")],
        Unavailable::Ambiguous(base.clone())
    );
}
