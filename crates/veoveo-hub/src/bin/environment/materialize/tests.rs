use super::*;
fn archive(entries: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut archive = tar::Builder::new(Vec::new());
    for (path, link, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        if let Some(link) = link {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name(link).unwrap();
            header.set_size(0);
        } else {
            header.set_size(bytes.len() as u64);
        }
        header.set_cksum();
        archive.append_data(&mut header, path, *bytes).unwrap();
    }
    archive.into_inner().unwrap()
}
#[test]
fn source_files_and_internal_links_extract_without_following_archive_links() {
    let root = tempfile::tempdir().unwrap();
    let bytes = archive(&[
        ("src/main.rs", None, b"fn main() {}"),
        ("entry.rs", Some("src/main.rs"), b""),
    ]);
    unpack(bytes.as_slice(), root.path()).unwrap();
    assert_eq!(
        fs::read(root.path().join("entry.rs")).unwrap(),
        b"fn main() {}"
    );
}
#[test]
fn planned_link_parent_chain_has_no_outside_side_effect() {
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join("staging");
    fs::create_dir(&staging).unwrap();
    let bytes = archive(&[
        ("a", Some("."), b""),
        ("a/link", Some("../outside"), b""),
        ("a/link/child", Some("."), b""),
    ]);
    assert!(unpack(bytes.as_slice(), &staging).is_err());
    assert!(!root.path().join("outside").exists());
    assert!(!staging.join("a").exists()); // all graph validation precedes link writes
}
#[test]
fn duplicate_paths_and_escaping_links_are_rejected() {
    for bytes in [
        archive(&[("file", None, b"one"), ("file", None, b"two")]),
        archive(&[("link", Some("../outside"), b"")]),
    ] {
        let root = tempfile::tempdir().unwrap();
        assert!(unpack(bytes.as_slice(), root.path()).is_err());
    }
}
