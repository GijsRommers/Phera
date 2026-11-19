//! A server keeps one startup snapshot across independent requests.

use super::super::Served;

#[test]
fn environment_snapshot_and_putenv_are_isolated_on_one_worker() {
    let served = Served::start_with_workers(
        "env",
        1,
        &[
            (".env", "PHERA_TEST_COLOR=green"),
            (
                "put.php",
                "<?php putenv('PHERA_TEST_COLOR=red'); \
         $_ENV['PHERA_TEST_COLOR'] = 'yellow'; $_SERVER['PHERA_TEST_COLOR'] = 'orange'; \
         echo getenv('PHERA_TEST_COLOR');",
            ),
            (
                "get.php",
                "<?php echo getenv('PHERA_TEST_COLOR'), $_ENV['PHERA_TEST_COLOR'], \
         $_SERVER['PHERA_TEST_COLOR'];",
            ),
        ],
    );
    assert_eq!(served.get("/put.php"), (200, "red".into()));
    std::fs::write(served.root.join(".env"), "PHERA_TEST_COLOR=blue").unwrap();
    assert_eq!(served.get("/get.php"), (200, "greengreengreen".into()));
}

#[test]
fn server_project_directory_and_file_options() {
    let files = [
        ("composer.json", "{}"),
        (".env", "PHERA_TEST_ROOT=project"),
        ("public/.env", "PHERA_TEST_ROOT=public"),
        ("public/first.cfg", "PHERA_TEST_ROOT=first"),
        ("public/second.cfg", "PHERA_TEST_ROOT=second"),
        ("public/index.php", "<?php echo getenv('PHERA_TEST_ROOT');"),
    ];
    for (label, flags, expected) in [
        ("project", vec![], "project"),
        ("none", vec!["--no-env-file"], ""),
        (
            "explicit",
            vec![
                "--env-file",
                "public/first.cfg",
                "--env-file",
                "public/second.cfg",
            ],
            "second",
        ),
    ] {
        let served = Served::start_in(label, 1, &files, "public", &flags);
        assert_eq!(served.get("/"), (200, expected.to_owned()));
        if label == "explicit" {
            assert_eq!(served.get("/first.cfg").0, 404);
            assert_eq!(served.get("/second.cfg").0, 404);
        }
    }
    let mut nested = files.to_vec();
    nested.push(("public/composer.json", "{}"));
    let served = Served::start_in("own-composer", 1, &nested, "public/index.php", &[]);
    assert_eq!(served.get("/"), (200, "public".into()));
}
