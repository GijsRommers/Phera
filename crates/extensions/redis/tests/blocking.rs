//! A blocking pop, compared with phpredis, that a second PHP process
//! satisfies by pushing on another connection while the first one waits.

use std::process::{Command, Stdio};

use phera_extension_api::testing::oracle::find_php;
use phera_extension_api::testing::scripts::{Comparison, Runtime};

#[test]
fn blocking_pop_is_satisfied_by_another_connection() {
    let Some(comparison) = Comparison::from_environment("redis-blocking") else {
        return;
    };
    let key = format!("phera:blocking:{}", std::process::id());
    let script = comparison.artifacts.join("pop.php");
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
    let pusher = format!(
        r#"$r=new Redis();$r->connect('127.0.0.1',16399);$r->select(14);
usleep(300000);$r->rPush('{key}',"from\0publisher");"#
    );
    let [php, phera] = [Runtime::Php, Runtime::Phera].map(|runtime| {
        let mut pushing = Command::new(find_php().unwrap())
            .args(["-n", "-r", &pusher])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let output = comparison.command(runtime, &script).output().unwrap();
        pushing.wait().unwrap();
        output
    });
    comparison.check("pop", &php, &phera).unwrap();
}
