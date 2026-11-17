//! The blocking command is satisfied by a second PHP process on a second connection.
use phera_extension_api::testing::oracle::find_php;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn blocking_pop_is_satisfied_by_another_connection() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        return;
    };
    let php = find_php().expect("PHP oracle");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/redis-blocking");
    std::fs::create_dir_all(&root).unwrap();
    let key = format!("phera:blocking:{}", std::process::id());
    let script = root.join("pop.php");
    std::fs::write(
        &script,
        format!(
            r#"<?php
$r=new Redis();$r->connect('127.0.0.1',16399);$r->select(14);
$r->del('{key}');var_dump($r->blPop(['{key}'],2));
"#
        ),
    )
    .unwrap();
    let mut outputs = Vec::new();
    for executable in [&php, Path::new(&binary)] {
        let source = format!(
            r#"$r=new Redis();$r->connect('127.0.0.1',16399);$r->select(14);
usleep(300000);$r->rPush('{key}',"from\0publisher");"#
        );
        let mut publisher = Command::new(&php)
            .args(["-n", "-r", &source])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let output = Command::new(executable).arg(&script).output().unwrap();
        publisher.wait().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        outputs.push(output.stdout);
    }
    assert_eq!(outputs[0], outputs[1]);
}
