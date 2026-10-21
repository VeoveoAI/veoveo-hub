use super::*;
use veoveo_hub::activation::{BuildOutput, MaterializedSource, ServiceRecipe, SnapshotIdentity};
use veoveo_types::BuildId;
fn request(root: &Path) -> BuildRequest {
    let build = BuildId::new();
    let identity = SnapshotIdentity {
        digest: Sha256Digest::from_bytes([1; 32]),
        size: 10,
    };
    let source = root.join(format!("sources/{build}"));
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("input"), b"first build output").unwrap();
    fs::write(
        source.join(".veoveo-source.json"),
        serde_json::to_vec(&identity).unwrap(),
    )
    .unwrap();
    BuildRequest {
        build,
        source: MaterializedSource {
            identity,
            directory: RelativePath::parse(format!("sources/{build}")).unwrap(),
        },
        recipe: ServiceRecipe {
            check: vec![],
            build: vec![CommandSpec {
                executable: "/bin/cp".into(),
                args: vec!["input".into(), "output".into()],
                cwd: RelativePath::parse(".").unwrap(),
            }],
            worker: WorkerSpec::Compiled {
                output: BuildOutput::Source {
                    path: RelativePath::parse("output").unwrap(),
                },
                args: vec![],
                cwd: RelativePath::parse(".").unwrap(),
            },
        },
    }
}
#[tokio::test]
async fn captured_build_is_reused_without_overwriting_executable_from_mutable_output() {
    let root = tempfile::tempdir().unwrap();
    let request = request(root.path());
    let result = run(root.path(), request.clone()).await.unwrap();
    let BuildResult::Ready { output, .. } = result else {
        panic!("expected captured output")
    };
    fs::write(
        root.path()
            .join(request.source.directory.as_str())
            .join("output"),
        b"later mutable output",
    )
    .unwrap();
    run(root.path(), request.clone()).await.unwrap();
    assert_eq!(
        fs::read(root.path().join(output.executable.as_str())).unwrap(),
        b"first build output"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(
            root.path().join(output.executable.as_str()),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    fs::write(root.path().join(output.executable.as_str()), b"tampered").unwrap();
    assert!(run(root.path(), request).await.is_err());
}
#[tokio::test]
async fn failed_check_keeps_bounded_owner_diagnostic_without_a_serving_output() {
    let root = tempfile::tempdir().unwrap();
    let mut request = request(root.path());
    request.recipe.check.push(CommandSpec {
        executable: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            "printf 'deliberately broken source' >&2; exit 2".into(),
        ],
        cwd: RelativePath::parse(".").unwrap(),
    });
    let BuildResult::Failed { failure, .. } = run(root.path(), request.clone()).await.unwrap()
    else {
        panic!("expected check failure")
    };
    assert_eq!(failure.exit_code, Some(2));
    assert!(failure.output.contains("deliberately broken source"));
    assert!(
        !root
            .path()
            .join(format!("builds/{}/service", request.build))
            .exists()
    );
    let retained: BuildResult = serde_json::from_slice(
        &fs::read(
            root.path()
                .join(format!("diagnostics/{}.json", request.build)),
        )
        .unwrap(),
    )
    .unwrap();
    let BuildResult::Failed {
        failure: stored, ..
    } = retained
    else {
        panic!("failure diagnostic lost")
    };
    assert_eq!(stored.output, failure.output);
}
