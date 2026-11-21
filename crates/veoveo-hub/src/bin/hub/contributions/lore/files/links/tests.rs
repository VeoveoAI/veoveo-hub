//! Synthetic native metadata fixtures; deployed Lore token/cache behavior is a separate probe.
use super::*;
use serde_json::json;
use veoveo_types::TenantId;

const PARENT: &str = "00000000000000000000000000000000";
const CHILD: &str = "11111111111111111111111111111111";

fn node() -> TreeNode {
    TreeNode {
        path: "library".into(),
        node_type: NodeType::Link as i32,
        address: Some(Address {
            context: hex::decode(CHILD).unwrap().into(),
            hash: vec![0xab; 32].into(),
        }),
        ..Default::default()
    }
}

fn event(source: &str, source_node: u64) -> serde_json::Value {
    json!({"tagName":"linkInfo", "data":{"entry":{
        "link":CHILD,"linkPath":"library","revision":"ab".repeat(32),
        "sourcePath":source,"sourceNode":source_node,
        "tracking":false,"branch":"22".repeat(16),"linkNode":42,"flags":0,
    }}})
}

#[test]
fn mount_url_requires_one_exact_revision_and_a_safe_nonroot_path() {
    let url = |tail: &str| format!("/lore/local/repositories/{PARENT}/links/{tail}");
    let sig = "ab".repeat(32);
    let (repository, revision, path) = named_mount(&url(&format!("@{sig}/library"))).unwrap();
    assert_eq!(repository.as_str(), PARENT);
    assert_eq!(revision.as_str(), sig);
    assert_eq!(path.tree_path(), "library");
    for tail in [
        "main/library",
        "@tip/library",
        &format!("@{sig}/"),
        &format!("@{sig}/a%2Fb"),
        &format!("@{sig}/../library"),
    ] {
        assert!(named_mount(&url(tail)).is_none(), "{tail}");
    }
}

#[test]
fn native_info_preserves_the_exact_mounted_subtree_and_crosschecks_identity() {
    let path = FilePath::decode("library").unwrap();
    for (source, source_node) in [("/", 0), ("robots/go2", 17), ("robots/100%25", 42)] {
        let bytes = serde_json::to_vec(&event(source, source_node)).unwrap();
        assert_eq!(decode_source_path(&bytes, &path, &node()).unwrap(), source);
    }
    for (source, source_node) in [
        ("/", 1),
        ("robots/go2", 0),
        ("/robots/go2", 17),
        ("robots//go2", 17),
        ("robots/../go2", 17),
        ("robots/go2/", 17),
        ("robots/./go2", 17),
    ] {
        assert!(
            decode_source_path(
                &serde_json::to_vec(&event(source, source_node)).unwrap(),
                &path,
                &node()
            )
            .is_err()
        );
    }
    for (field, forged) in [
        ("link", json!(PARENT)),
        ("revision", json!("cd".repeat(32))),
        ("linkPath", json!("another")),
    ] {
        let mut value = event("robots/go2", 17);
        value["data"]["entry"][field] = forged;
        assert!(decode_source_path(&serde_json::to_vec(&value).unwrap(), &path, &node()).is_err());
    }
    let mut changed = node();
    changed.address.as_mut().unwrap().hash = vec![0xcd; 32].into();
    assert!(
        decode_source_path(
            &serde_json::to_vec(&event("/", 0)).unwrap(),
            &path,
            &changed
        )
        .is_err()
    );
}

#[test]
fn native_info_requires_exactly_one_matching_event() {
    let path = FilePath::decode("library").unwrap();
    let value = serde_json::to_string(&event("/", 0)).unwrap();
    assert!(decode_source_path(b"", &path, &node()).is_err());
    assert!(decode_source_path(format!("{value}\n{value}\n").as_bytes(), &path, &node()).is_err());
    let unrelated = "{\"tagName\":\"complete\",\"data\":{\"status\":0}}";
    assert_eq!(
        decode_source_path(format!("{unrelated}\n{value}\n").as_bytes(), &path, &node()).unwrap(),
        "/"
    );
}

#[test]
fn parent_access_does_not_admit_the_child_and_revocation_is_not_cached() {
    let mut plane = super::super::super::tests::plane();
    let mut value = plane.extensions.remove("lore").unwrap();
    let source = |value| serde_json::from_value::<Lore>(value).unwrap();
    let principal = PrincipalId::parse("native-alice").unwrap();
    let tenant = TenantId::parse("tenant-a").unwrap();
    let caller = Caller {
        tenant: &tenant,
        principal: &principal,
    };
    let section = source(value.clone());
    let deployment = section.sole_deployment().unwrap();
    section
        .readable(
            &deployment.id,
            &RepositoryId::parse(PARENT).unwrap(),
            caller,
        )
        .unwrap();
    assert_eq!(
        authorized_target(&section, deployment, caller, &node())
            .unwrap_err()
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
    value["assignments"].as_array_mut().unwrap().push(json!({"deployment":"source","repository":CHILD,"tenant":"tenant-a","principal":"native-alice","actions":[]}));
    let section = source(value.clone());
    let deployment = section.sole_deployment().unwrap();
    assert_eq!(
        authorized_target(&section, deployment, caller, &node())
            .unwrap()
            .0
            .as_str(),
        CHILD
    );
    value["assignments"].as_array_mut().unwrap().pop();
    let section = source(value);
    assert!(
        authorized_target(
            &section,
            section.sole_deployment().unwrap(),
            caller,
            &node()
        )
        .is_err()
    );
}

#[test]
fn native_clones_keep_tokens_repository_scoped_and_materialize_no_files() {
    let root = tempfile::tempdir().unwrap();
    let revision = Revision::parse("ab".repeat(32)).unwrap();
    for (repository, token, checkout) in [
        (PARENT, "parent-only-token", "parent"),
        (CHILD, "child-only-token", "target"),
    ] {
        let url = format!("grpcs://source.example/{repository}")
            .parse()
            .unwrap();
        let command = clone_command(root.path(), token, &url, &revision, checkout);
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(&args[..4], &["--access-token", token, "--cache", "clone"]);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--revision", revision.as_str()])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--view" && pair[1].ends_with("exclude.view"))
        );
        assert!(args.contains(&url.as_str()));
        assert!(!args.contains(&if repository == PARENT {
            "child-only-token"
        } else {
            "parent-only-token"
        }));
    }
    let offline = tree_read::lore(root.path(), "");
    assert!(
        !offline
            .as_std()
            .get_args()
            .any(|arg| arg == "--access-token")
    );
}

#[tokio::test]
async fn native_output_is_bounded_before_parsing() {
    assert!(bounded_output(&b"12345678"[..], 7).await.is_err());
    assert_eq!(
        bounded_output(&b"1234567"[..], 7).await.unwrap(),
        b"1234567"
    );
}

#[tokio::test]
async fn mount_lookup_walks_parent_directories_but_never_enters_an_ancestor_link() {
    struct OwnTree {
        ancestor: NodeType,
        seen: std::sync::Mutex<Vec<Option<String>>>,
    }
    impl Tree for OwnTree {
        fn revision(&self) -> Option<String> {
            None
        }
        fn folder<'a>(
            &'a self,
            folder: Option<&'a str>,
        ) -> futures::future::BoxFuture<'a, Result<Vec<TreeNode>>> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(folder.map(str::to_owned));
                match folder {
                    None => Ok(vec![TreeNode {
                        path: "assets".into(),
                        node_type: self.ancestor as i32,
                        ..Default::default()
                    }]),
                    Some("assets") if self.ancestor == NodeType::Directory => {
                        let mut link = node();
                        link.path = "assets/library".into();
                        Ok(vec![link])
                    }
                    _ => panic!(
                        "lookup entered a linked repository without an independent mount resolution"
                    ),
                }
            })
        }
    }
    let path = FilePath::decode("assets/library").unwrap();
    let ordinary = OwnTree {
        ancestor: NodeType::Directory,
        seen: Default::default(),
    };
    assert_eq!(
        mount_node(&ordinary, &path).await.unwrap().unwrap().path,
        "assets/library"
    );
    assert_eq!(
        *ordinary.seen.lock().unwrap(),
        [None, Some("assets".into())]
    );
    let linked = OwnTree {
        ancestor: NodeType::Link,
        seen: Default::default(),
    };
    assert!(mount_node(&linked, &path).await.unwrap().is_none());
    assert_eq!(*linked.seen.lock().unwrap(), [None]);
}
