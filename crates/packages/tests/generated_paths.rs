//! Reject existing output links before install or dump-autoload can follow them.
//!
//! An existing `vendor/composer/installed.php` link must never redirect an installer write.

#[cfg(unix)]
#[test]
fn generated_output_symlinks_cannot_redirect_installer_writes() {
    use phera_packages::{Options, dump_autoload, install};
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let root = directory.path().join("project");
    let protected = directory.path().join("protected");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&protected).unwrap();
    fs::write(protected.join("installed.php"), "protected data").unwrap();
    fs::write(root.join("composer.json"), "{}").unwrap();
    fs::write(
        root.join("composer.lock"),
        json!({"content-hash":"fixture", "packages":[]}).to_string(),
    )
    .unwrap();
    let options = Options {
        cache_dir: Some(root.join("cache")),
        no_scripts: true,
        ..Options::default()
    };
    for path in [
        "composer",
        "composer/installed.php",
        "composer/include_paths.php",
        "autoload.php",
    ] {
        let vendor = root.join("vendor");
        let link = vendor.join(path);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        let target = if path == "composer" {
            protected.clone()
        } else {
            protected.join("installed.php")
        };
        symlink(target, &link).unwrap();
        assert!(
            install(&root, &options)
                .unwrap_err()
                .to_string()
                .contains("symlink")
        );
        assert!(
            dump_autoload(&root, &options)
                .unwrap_err()
                .to_string()
                .contains("symlink")
        );
        assert_eq!(
            fs::read_to_string(protected.join("installed.php")).unwrap(),
            "protected data"
        );
        fs::remove_dir_all(vendor).unwrap();
    }
}
