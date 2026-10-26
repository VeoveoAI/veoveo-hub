//! Versions of synthetic specs: the same inputs name the same version whatever their
//! order, object identity or names, a change of anything a sandbox gets names another,
//! and what's new lists the release note when it differs, the image and the network
//! endpoints gained or lost. No gateway is involved.
use std::collections::HashMap;

use openshell_sdk::raw::proto as p;

use super::{
    super::{
        ComputerProvisionSpec,
        spec::PersistenceBackend,
        test_fixture::{config, model_secret},
    },
    *,
};

fn policy(yaml: &str) -> p::sandbox::v1::SandboxPolicy {
    openshell_policy::validate_and_canonicalize_sandbox_policy(
        openshell_policy::parse_sandbox_policy(yaml).unwrap(),
    )
    .unwrap()
}

const HUB_ONLY: &str = "version: 1
network_policies:
  hub:
    endpoints:
    - host: hub.example
      port: 443
      path: /mcp/computer
      protocol: rest
      enforcement: enforce
      access: read-only
    binaries:
    - path: /usr/local/bin/opencode
";

const HUB_AND_MODEL: &str = "version: 1
network_policies:
  model:
    endpoints:
    - host: model.example
      port: 443
      protocol: rest
      enforcement: enforce
      access: read-only
    binaries:
    - path: /usr/local/bin/opencode
  hub:
    endpoints:
    - host: hub.example
      port: 443
      path: /computer/*/status
      protocol: rest
      enforcement: enforce
      access: read-only
    binaries:
    - path: /**
";

fn template(
    image: &str,
    environment: &[(&str, &str)],
    annotations: &[(&str, &str)],
) -> p::SandboxWorkloadTemplate {
    p::SandboxWorkloadTemplate {
        metadata: Some(p::ObjectMeta {
            id: uuid::Uuid::new_v4().to_string(),
            name: "computer-v1".into(),
            workspace: "installation".into(),
            resource_version: 1,
            annotations: annotations
                .iter()
                .map(|(key, value)| ((*key).into(), (*value).into()))
                .collect(),
            ..Default::default()
        }),
        spec: Some(p::SandboxWorkloadTemplateSpec {
            workload: Some(p::SandboxWorkloadConfig {
                image: image.into(),
                environment: environment
                    .iter()
                    .map(|(key, value)| ((*key).into(), (*value).into()))
                    .collect::<HashMap<_, _>>(),
                ..Default::default()
            }),
            ..Default::default()
        }),
    }
}

fn spec(policy_yaml: &str, template: &p::SandboxWorkloadTemplate) -> ComputerProvisionSpec {
    let config = config();
    ComputerProvisionSpec::new(
        config.clone(),
        model_secret(&config),
        "https://hub.example/mcp/computer".into(),
        PersistenceBackend::Kubernetes,
        &policy(policy_yaml),
        template,
    )
}

#[test]
fn a_version_names_what_a_sandbox_gets_and_nothing_else() {
    let environment = [("A", "1"), ("B", "2"), ("C", "3"), ("D", "4")];
    let base = spec(
        HUB_ONLY,
        &template("registry/computer@sha256:1", &environment, &[]),
    );
    let version = base.version().unwrap();
    assert_eq!(version.as_str().len(), 12);
    assert!(
        version
            .as_str()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    );

    // Another object, name, environment order, handbook, policy file path, connection
    // limit or collection setting.
    let mut reversed = environment;
    reversed.reverse();
    let mut same = spec(
        HUB_ONLY,
        &template("registry/computer@sha256:1", &reversed, &[]),
    );
    same.config.handbook = Some("Edited guidance".into());
    same.config.policy_file = "/etc/veoveo/hub/computer-v2.yaml".into();
    same.config.template.name = "computer-v2".into();
    same.config.forwarding_connection_limit = std::num::NonZeroUsize::new(7).unwrap();
    same.config.collect_after_seconds = std::num::NonZeroU64::new(60);
    assert_eq!(
        same.version().unwrap(),
        version,
        "the same inputs name the same version"
    );
    let noted = spec(
        HUB_ONLY,
        &template(
            "registry/computer@sha256:1",
            &environment,
            &[(RELEASE_NOTE, "Notes only")],
        ),
    );
    assert_eq!(
        noted.version().unwrap(),
        version,
        "a release note describes a version; it is not one"
    );

    // Anything a sandbox gets from the installation.
    for other in [
        spec(
            HUB_ONLY,
            &template("registry/computer@sha256:2", &environment, &[]),
        ),
        spec(
            HUB_AND_MODEL,
            &template("registry/computer@sha256:1", &environment, &[]),
        ),
        spec(
            HUB_ONLY,
            &template("registry/computer@sha256:1", &[("A", "changed")], &[]),
        ),
        {
            let mut command = base.clone();
            command.config.command = vec!["/usr/local/bin/other".into()];
            command
        },
    ] {
        assert_ne!(other.version().unwrap(), version);
    }
}

#[test]
fn whats_new_lists_the_note_the_image_and_the_endpoints() {
    let older = spec(HUB_ONLY, &template("registry/computer@sha256:1", &[], &[]));
    let newer = spec(
        HUB_AND_MODEL,
        &template(
            "registry/computer@sha256:2",
            &[],
            &[(
                RELEASE_NOTE,
                "\n  Faster builds\nThe image adds a compiler.\n",
            )],
        ),
    );
    assert_eq!(newer.summary().as_deref(), Some("Faster builds"));
    assert_eq!(older.summary(), None);
    let changes = whats_new(&older, &newer).unwrap();
    assert_eq!(
        changes.note.as_deref(),
        Some("Faster builds\nThe image adds a compiler.")
    );
    assert_eq!(
        changes.image,
        Some(ImageChange {
            from: "registry/computer@sha256:1".into(),
            to: "registry/computer@sha256:2".into(),
        })
    );
    assert_eq!(
        changes.network_rules_added,
        [
            NetworkRule {
                name: "hub".into(),
                endpoints: vec!["hub.example:443/computer/*/status".into()],
            },
            NetworkRule {
                name: "model".into(),
                endpoints: vec!["model.example:443".into()],
            },
        ]
    );
    assert_eq!(
        changes.network_rules_removed,
        [NetworkRule {
            name: "hub".into(),
            endpoints: vec!["hub.example:443/mcp/computer".into()],
        }]
    );
    // Without a note, the mechanical differences remain.
    let back = whats_new(&newer, &older).unwrap();
    assert_eq!(back.note, None);
    assert_eq!(back.network_rules_added, changes.network_rules_removed);
    assert_eq!(
        serde_json::to_value(&back).unwrap()["networkRulesRemoved"][1]["name"],
        "model"
    );
    assert_eq!(whats_new(&older, &older).unwrap(), WhatsNew::default());
}

#[test]
fn a_note_is_listed_only_when_it_differs_from_the_running_versions() {
    let noted = |policy_yaml: &str, note: &str| {
        spec(
            policy_yaml,
            &template("registry/computer@sha256:1", &[], &[(RELEASE_NOTE, note)]),
        )
    };
    let running = noted(HUB_ONLY, "Faster builds");

    // Only the policy changes: the template's note is not new, its endpoints are.
    let policy = noted(HUB_AND_MODEL, "Faster builds");
    assert_ne!(policy.version().unwrap(), running.version().unwrap());
    let changes = whats_new(&running, &policy).unwrap();
    assert_eq!(changes.note, None, "an unchanged note is not new");
    assert_eq!(changes.network_rules_added.len(), 2);
    assert_eq!(changes.network_rules_removed.len(), 1);

    // Only the Hub's Computer settings change: nothing a person is told differs.
    let mut settings = running.clone();
    settings.config.command = vec!["/usr/local/bin/other".into()];
    assert_ne!(settings.version().unwrap(), running.version().unwrap());
    assert_eq!(whats_new(&running, &settings).unwrap(), WhatsNew::default());

    // Another note is new.
    assert_eq!(
        whats_new(&running, &noted(HUB_AND_MODEL, "Smaller image"))
            .unwrap()
            .note
            .as_deref(),
        Some("Smaller image")
    );
}
