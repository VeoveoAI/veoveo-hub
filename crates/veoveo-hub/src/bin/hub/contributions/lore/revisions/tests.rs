//! The routes' answers against a stand-in for Lore, whose records are synthetic. Connecting to Lore
//! and issuing the token are not exercised here; the deployed probe reads real revisions.
use std::collections::BTreeMap;

use lore_proto::lore::{
    model::v1::{Address, BranchPoint, RevisionIdentifier},
    thin_client::v1::revision::Parent,
};

use super::*;

const MAIN: &[u8] = &[1; 16];
const OTHER: &[u8] = &[2; 16];
const LINKED: &str = "0123456789abcdef0123456789abcdef";

fn hash(byte: u8) -> Vec<u8> {
    vec![byte; 32]
}

fn named(byte: u8) -> Revision {
    Revision::parse(hex::encode(hash(byte))).unwrap()
}

fn record(byte: u8, parent: Option<(u8, &[u8])>) -> NativeRevision {
    NativeRevision {
        signature: hash(byte).into(),
        identifier: Some(RevisionIdentifier {
            branch_id: MAIN.to_vec().into(),
            number: u64::from(byte),
        }),
        commit_message: format!("revision {byte}"),
        timestamp: 1_700_000_000_000 + u64::from(byte),
        committed_by: "someone".to_owned(),
        number: u64::from(byte),
        parent_self: parent.map(|(byte, branch)| Parent {
            signature: hash(byte).into(),
            identifier: Some(RevisionIdentifier {
                branch_id: branch.to_vec().into(),
                number: u64::from(byte),
            }),
        }),
        ..Default::default()
    }
}

/// Lore holding these revisions, with the first as the tip of every branch.
#[derive(Default)]
struct StandIn {
    records: Vec<NativeRevision>,
    changes: Vec<DiffChange>,
    nodes: Vec<TreeNode>,
}

impl Log for StandIn {
    fn branches(&self) -> BoxFuture<'_, Result<Vec<NativeBranch>>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn tip<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Result<Option<Revision>>> {
        Box::pin(async {
            Ok(self
                .records
                .first()
                .map(|record| Revision::parse(hex::encode(&record.signature)).unwrap()))
        })
    }

    fn revision<'a>(&'a self, revision: &'a Revision) -> BoxFuture<'a, Result<NativeRevision>> {
        Box::pin(async move {
            self.records
                .iter()
                .find(|record| hex::encode(&record.signature) == revision.as_str())
                .cloned()
                .context("no such revision")
        })
    }

    fn diff<'a>(
        &'a self,
        _: &'a Revision,
        _: &'a Revision,
    ) -> BoxFuture<'a, Result<(Vec<DiffChange>, bool)>> {
        Box::pin(async { Ok((self.changes.clone(), false)) })
    }

    fn everything<'a>(&'a self, _: &'a Revision) -> BoxFuture<'a, Result<Vec<TreeNode>>> {
        Box::pin(async { Ok(self.nodes.clone()) })
    }
}

fn line() -> StandIn {
    StandIn {
        records: vec![
            record(5, Some((4, MAIN))),
            record(4, Some((3, MAIN))),
            record(3, Some((2, MAIN))),
            record(2, Some((1, OTHER))),
        ],
        ..Default::default()
    }
}

fn numbers(records: &[NativeRevision]) -> Vec<u64> {
    records.iter().map(|record| record.number).collect()
}

#[tokio::test]
async fn a_page_of_history_runs_back_from_the_tip_and_names_the_next() {
    let (records, next) = walk(&line(), "main", None, 2).await.unwrap();
    assert_eq!(numbers(&records), [5, 4]);
    assert_eq!(next, Some(named(3)));
}

#[tokio::test]
async fn the_next_page_starts_where_it_was_named() {
    let (records, next) = walk(&line(), "main", Some(named(3)), 2).await.unwrap();
    assert_eq!(numbers(&records), [3, 2]);
    assert_eq!(next, None, "its parent is on the branch it forked from");
}

#[tokio::test]
async fn a_branch_without_a_revision_has_no_history() {
    let (records, next) = walk(&StandIn::default(), "main", None, 2).await.unwrap();
    assert!(records.is_empty());
    assert_eq!(next, None);
}

#[test]
fn a_revision_keeps_a_parent_on_another_branch_and_says_what_it_merged() {
    let mut merge = record(2, Some((1, OTHER)));
    merge.parent_other = Some(Parent {
        signature: hash(9).into(),
        identifier: Some(RevisionIdentifier {
            branch_id: OTHER.to_vec().into(),
            number: 9,
        }),
    });
    let names = BTreeMap::from([(OTHER.to_vec(), "probe".to_owned())]);
    let views = revision_views(vec![merge], &[], &names).unwrap();
    assert_eq!(views[0].parent, Some(named(1)));
    assert_eq!(
        views[0].merged,
        Some(Merged {
            revision: named(9),
            branch: Some("probe".to_owned()),
        })
    );
    assert_eq!(views[0].message, "revision 2");
    assert_eq!(views[0].at, 1_700_000_000_002);
    assert_eq!(views[0].branch_id, Some(hex::encode(MAIN)));
}

#[test]
fn someone_who_never_signed_in_is_named_as_lore_holds_them() {
    let shown = by("someone", &[]);
    assert_eq!(shown.name.as_deref(), Some("someone"));
    assert_eq!(shown.email, None);
}

fn address(byte: u8) -> Address {
    Address {
        hash: hash(byte).into(),
        context: vec![7; 8].into(),
    }
}

fn file(path: &str, action: Action, from: Option<u8>, to: Option<u8>) -> DiffChange {
    DiffChange {
        path: path.to_owned(),
        action: action as i32,
        node_type: NodeType::File as i32,
        content_from: from.map(address),
        content_to: to.map(address),
        ..Default::default()
    }
}

fn link_move(path: &str) -> DiffChange {
    let pinned = |byte: u8| Address {
        hash: hash(byte).into(),
        context: hex::decode(LINKED).unwrap().into(),
    };
    DiffChange {
        path: path.to_owned(),
        action: Action::Keep as i32,
        node_type: NodeType::Link as i32,
        content_from: Some(pinned(1)),
        content_to: Some(pinned(2)),
        link_repository_index: 1,
        tracking: true,
        ..Default::default()
    }
}

fn no_one(_: &RepositoryId) -> Option<String> {
    None
}

#[test]
fn a_change_says_what_happened_to_a_file() {
    let views = change_views(
        vec![
            file("added", Action::Add, None, Some(1)),
            file("gone", Action::Delete, Some(1), None),
            file("edited", Action::Keep, Some(1), Some(2)),
            DiffChange {
                path_from: "old".to_owned(),
                ..file("new", Action::Move, Some(1), Some(1))
            },
        ],
        &no_one,
    );
    let actions = views.iter().map(|view| view.action).collect::<Vec<_>>();
    assert_eq!(
        actions,
        [
            ChangeAction::Added,
            ChangeAction::Removed,
            ChangeAction::Changed,
            ChangeAction::Moved
        ]
    );
    assert_eq!(views[0].address_from, None);
    assert!(views[0].address_to.as_ref().unwrap().contains('-'));
    assert_eq!(views[3].from_path.as_deref(), Some("old"));
}

#[test]
fn folders_what_lies_inside_a_link_and_the_unchanged_are_left_out() {
    let views = change_views(
        vec![
            DiffChange {
                node_type: NodeType::Directory as i32,
                ..file("folder", Action::Keep, Some(1), Some(2))
            },
            DiffChange {
                link_repository_index: 1,
                ..file("team/flow.json", Action::Keep, Some(1), Some(2))
            },
            file("same", Action::Keep, Some(1), Some(1)),
        ],
        &no_one,
    );
    assert!(views.is_empty());
}

#[test]
fn a_link_named_twice_is_one_change() {
    let views = change_views(vec![link_move("team"), link_move("team")], &no_one);
    assert_eq!(views.len(), 1);
}

#[test]
fn a_link_moves_with_its_revisions_for_a_person_who_reaches_its_repository() {
    let reach = |_: &RepositoryId| Some("team-a".to_owned());
    let views = change_views(vec![link_move("team")], &reach);
    let link = views[0].link.as_ref().unwrap();
    assert!(link.reached);
    assert_eq!(link.title.as_deref(), Some("team-a"));
    assert_eq!(link.repository.as_ref().unwrap().as_str(), LINKED);
    assert_eq!(link.from, Some(named(1)));
    assert_eq!(link.to, Some(named(2)));
    assert!(link.tracking);
    assert_eq!(views[0].kind, ChangeKind::Link);
    assert_eq!(views[0].action, ChangeAction::Changed);
}

#[test]
fn a_person_without_a_grant_learns_only_that_the_link_changed() {
    let views = change_views(vec![link_move("team")], &no_one);
    assert_eq!(views[0].path, "team");
    assert_eq!(views[0].link, Some(LinkView::default()));
    assert_eq!(views[0].address_from, None);
    assert_eq!(views[0].address_to, None);
    let answered = serde_json::to_value(&views[0]).unwrap();
    assert_eq!(
        answered["link"],
        serde_json::json!({"reached": false, "tracking": false}),
        "nothing of the linked repository"
    );
}

#[tokio::test]
async fn a_revision_is_compared_with_its_parent_unless_asked() {
    let log = StandIn {
        changes: vec![file("edited", Action::Keep, Some(1), Some(2))],
        ..line()
    };
    let (against, views, truncated) = changes_of(&log, &named(5), None, &no_one).await.unwrap();
    assert_eq!(against, Some(named(4)));
    assert_eq!(views.len(), 1);
    assert!(!truncated);
    let (against, _, _) = changes_of(&log, &named(5), Some(named(2)), &no_one)
        .await
        .unwrap();
    assert_eq!(against, Some(named(2)));
}

#[tokio::test]
async fn the_first_revision_answers_everything_as_added() {
    let node = |path: &str, node_type: NodeType| TreeNode {
        path: path.to_owned(),
        node_type: node_type as i32,
        address: Some(address(1)),
        ..Default::default()
    };
    let log = StandIn {
        records: vec![record(1, None)],
        nodes: vec![
            node("folder", NodeType::Directory),
            node("folder/a.txt", NodeType::File),
            node("team", NodeType::Link),
            node("team/inside.txt", NodeType::File),
        ],
        ..Default::default()
    };
    let (against, views, _) = changes_of(&log, &named(1), None, &no_one).await.unwrap();
    assert_eq!(against, None);
    let paths = views
        .iter()
        .map(|view| view.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["folder/a.txt", "team"]);
    assert!(views.iter().all(|view| view.action == ChangeAction::Added));
}

#[test]
fn branches_leave_out_the_deleted_and_say_where_each_forked() {
    let branch =
        |name: &str, latest: Vec<u8>, deleted: bool, stack: Vec<BranchPoint>| NativeBranch {
            name: name.to_owned(),
            latest: latest.into(),
            deleted,
            stack,
            created: 1_700_000_000_000,
            ..Default::default()
        };
    let views = branch_views(vec![
        branch(
            "probe",
            hash(3),
            false,
            vec![BranchPoint {
                branch_id: MAIN.to_vec().into(),
                revision_signature: hash(2).into(),
            }],
        ),
        branch("empty", vec![0; 32], false, Vec::new()),
        branch("gone", hash(4), true, Vec::new()),
    ]);
    let names = views
        .iter()
        .map(|view| view.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["empty", "probe"]);
    assert_eq!(views[0].latest, None);
    assert_eq!(views[1].latest, Some(named(3)));
    assert_eq!(views[1].forked_at, Some(named(2)));
}

#[test]
fn a_diff_too_large_for_lore_is_said_to_be() {
    let problem = failed(anyhow::Error::new(Answered {
        code: tonic::Code::ResourceExhausted,
        message: "too large".to_owned(),
    }));
    assert_eq!(problem.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn what_cannot_be_a_revision_is_not_there() {
    let problem = revision_named("not-a-signature").unwrap_err();
    assert_eq!(problem.status(), StatusCode::NOT_FOUND);
}
