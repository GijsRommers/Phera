//! Hostile archive regression tests; `../outside` must never leave staging.

use super::extract;
use std::fs;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

#[test]
fn rejects_zip_traversal_absolute_paths_and_escaping_symlinks() {
    for name in [
        "../outside",
        "/absolute",
        "C:/windows",
        "root/../../outside",
    ] {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let target = directory.path().join("extract");
        fs::create_dir(&target).unwrap();
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"hostile").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert!(extract(&bytes, "zip", &target).is_err(), "{name}");
        assert!(!directory.path().join("outside").exists());
    }
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .add_symlink("root/link", "../../outside", SimpleFileOptions::default())
        .unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    assert!(extract(&bytes, "zip", directory.path()).is_err());
}

#[test]
fn rejects_tar_traversal_and_absurd_sizes() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_mode(0o644);
    header.as_mut_bytes()[..10].copy_from_slice(b"../outside");
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(&[0; 1024]);
    assert!(extract(&bytes, "tar", directory.path()).is_err());
    let mut total = 0;
    assert!(super::check_size(super::MAX_FILE + 1, &mut total).is_err());
    let mut total = super::MAX_PACKAGE;
    assert!(super::check_size(1, &mut total).is_err());
}

#[test]
fn rejects_giant_tar_metadata_before_allocating_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::GNULongName);
    header.set_path("././@LongLink").unwrap();
    header.set_size(1024 * 1024 + 1);
    header.set_mode(0o644);
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(&[0; 1024]);
    let error = extract(&bytes, "tar", directory.path())
        .unwrap_err()
        .to_string();
    assert!(error.contains("metadata exceeds"), "{error}");
}

#[test]
fn rejects_a_zip_with_an_absurd_declared_file_size() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("payload", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"x").unwrap();
    let mut bytes = archive.finish().unwrap().into_inner();
    let size = (super::MAX_FILE as u32 + 1).to_le_bytes();
    bytes[22..26].copy_from_slice(&size);
    let central = bytes
        .windows(4)
        .position(|part| part == b"PK\x01\x02")
        .unwrap();
    bytes[central + 24..central + 28].copy_from_slice(&size);
    let error = extract(&bytes, "zip", directory.path())
        .unwrap_err()
        .to_string();
    assert!(error.contains("safety limits"), "{error}");
    assert!(!directory.path().join("payload").exists());
}
