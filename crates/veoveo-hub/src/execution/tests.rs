use std::collections::BTreeMap;

use chrono::{TimeDelta, Utc};
use veoveo_hub_sdk::{DeploymentId, ResolvedComponent, ServerSlug};

use super::*;
use crate::registration::RegistrationSnapshot;

fn selected(deployment: DeploymentId) -> ResolvedComponent {
    ResolvedComponent::Deployment { deployment }
}

#[test]
fn a_server_routes_to_its_newest_registered_deployment() {
    let now = Utc::now();
    let deployment = |slug: &str, age: i64| {
        let mut server = crate::test_catalog::registered_server(slug, "https://server.test/mcp");
        server.registered_at = now - TimeDelta::minutes(age);
        server
    };
    let (old, new, other) = (
        deployment("ledger", 30),
        deployment("ledger", 1),
        deployment("relay", 90),
    );
    let (old_id, new_id, other_id) = (old.deployment, new.deployment, other.deployment);
    let registrations = RegistrationSnapshot::new([old, new, other]);

    let admitted = selection::route(&registrations, None, &BTreeMap::new());
    assert_eq!(
        admitted.composition.components(),
        &BTreeMap::from([
            (slug("ledger"), selected(new_id)),
            (slug("relay"), selected(other_id))
        ]),
        "registering is serving: no enrollment, and the newest of several wins"
    );
    assert!(admitted.unavailable.is_empty());
    let namespace = &registrations.namespaces()[&slug("ledger")];
    assert_eq!(
        (namespace.routed, namespace.set_aside.clone()),
        (new_id, vec![old_id])
    );
    assert!(
        registrations.namespaces()[&slug("relay")]
            .set_aside
            .is_empty()
    );

    // The same registrations route the same way, so retained captures stay equal.
    assert_eq!(
        selection::route(&registrations, None, &BTreeMap::new())
            .composition
            .revision(),
        admitted.composition.revision()
    );
}

#[test]
fn a_server_whose_newest_deployment_ended_routes_to_an_older_live_one() {
    let now = Utc::now();
    let deployment = |age: i64, orphaned: bool| {
        let mut server =
            crate::test_catalog::registered_server("ledger", "https://ledger.test/mcp");
        server.registered_at = now - TimeDelta::minutes(age);
        server.orphaned = orphaned;
        server
    };
    // The newest deployment's process ended: housekeeping marked its registration.
    let (ended, live) = (deployment(1, true), deployment(30, false));
    let (ended_id, live_id) = (ended.deployment, live.deployment);
    let registrations = RegistrationSnapshot::new([ended, live]);
    let namespace = &registrations.namespaces()[&slug("ledger")];
    assert_eq!(
        (namespace.routed, namespace.set_aside.clone()),
        (live_id, vec![ended_id])
    );
    assert_eq!(
        selection::route(&registrations, None, &BTreeMap::new())
            .composition
            .components()[&slug("ledger")],
        selected(live_id)
    );

    // With every deployment ended, the newest still routes, as before, until the grace
    // period removes its registration or its process registers again.
    let (newest, older) = (deployment(1, true), deployment(30, true));
    let newest_id = newest.deployment;
    let registrations = RegistrationSnapshot::new([newest, older]);
    assert_eq!(
        registrations.namespaces()[&slug("ledger")].routed,
        newest_id
    );
}

#[test]
fn instances_take_a_server_away_from_its_registered_deployment() {
    let namespace = crate::test_catalog::registered_server("ledger", "https://ledger.test/mcp");
    let mut instance =
        crate::test_catalog::registered_server("ledger", "https://instance.test/mcp");
    instance.branch = Some(branch("dev"));
    let instance_id = instance.deployment;
    let registrations = RegistrationSnapshot::new([namespace, instance]);
    assert!(
        registrations.namespaces()[&slug("ledger")]
            .set_aside
            .is_empty(),
        "an instance is not one of the slug's registered deployments"
    );
    let admitted = selection::route(&registrations, Some(&branch("dev")), &BTreeMap::new());
    assert_eq!(
        admitted.composition.components()[&slug("ledger")],
        selected(instance_id)
    );
    let admitted = selection::route(&registrations, Some(&branch("main")), &BTreeMap::new());
    assert_eq!(
        admitted.composition.components()[&slug("ledger")],
        ResolvedComponent::Unavailable {},
        "a server with instances never falls back to its other deployment"
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
    let routed = BTreeMap::from([
        (slug("observatory"), selected(DeploymentId::new())),
        (slug("relay"), selected(DeploymentId::new())),
    ]);
    let registrations = RegistrationSnapshot::new([dev, feature]);
    let base = branch("dev");
    let resolve = |base: Option<&BranchName>, selection: &[(&str, &str)]| {
        let selection = selection
            .iter()
            .map(|(server, name)| (slug(server), branch(name)))
            .collect();
        select_instances(routed.clone(), &registrations, base, &selection)
    };

    let unselected = resolve(Some(&base), &[]);
    assert_eq!(
        unselected.composition.components()[&slug("observatory")],
        selected(dev_id)
    );
    assert_eq!(
        unselected.composition.components()[&slug("relay")],
        routed[&slug("relay")],
        "a server without instances keeps its routed deployment"
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
    let ambiguous = select_instances(routed.clone(), &twice, Some(&base), &BTreeMap::new());
    assert_eq!(
        ambiguous.unavailable[&slug("observatory")],
        Unavailable::Ambiguous(base.clone())
    );
    let explicitly_ambiguous = select_instances(
        routed,
        &twice,
        None,
        &BTreeMap::from([(slug("observatory"), base.clone())]),
    );
    assert_eq!(
        explicitly_ambiguous.unavailable[&slug("observatory")],
        Unavailable::Ambiguous(base)
    );
}
