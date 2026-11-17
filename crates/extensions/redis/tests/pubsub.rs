//! A real PHP publisher is a second process; each subscriber must run PHP callbacks.
use phera_extension_api::testing::oracle::find_php;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn callbacks_and_unsubscribe_match_php() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        return;
    };
    let php = find_php().expect("PHP oracle");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = root.join("../../../target/redis-pubsub");
    std::fs::create_dir_all(&output).unwrap();
    for mode in ["subscribe", "psubscribe", "ssubscribe"] {
        let channel = format!("phera:pubsub:{}:{mode}", std::process::id());
        let unsubscribe = match mode {
            "psubscribe" => "punsubscribe",
            "ssubscribe" => "sunsubscribe",
            _ => "unsubscribe",
        };
        let callback = if mode == "psubscribe" {
            "$r,$pattern,$channel,$message"
        } else {
            "$r,$channel,$message"
        };
        let source = format!(
            r#"<?php
$r=new Redis();$r->connect('127.0.0.1',16399);
var_dump($r->{mode}(['{channel}'],function({callback}) {{
var_dump($channel,$message);var_dump($r->{unsubscribe}(['{channel}']));
}}));
"#
        );
        let file = output.join(format!("{mode}.php"));
        std::fs::write(&file, source).unwrap();
        let mut outputs = Vec::new();
        for executable in [&php, Path::new(&binary)] {
            let publish = if mode == "ssubscribe" {
                "SPUBLISH"
            } else {
                "PUBLISH"
            };
            let publisher = format!(
                r#"$r=new Redis();$r->connect('127.0.0.1',16399);
usleep(500000);for($i=0;$i<30;$i++){{$r->rawCommand('{publish}','{channel}',"hello\0redis");
usleep(50000);}}"#
            );
            let mut child = Command::new(&php)
                .args(["-n", "-r", &publisher])
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            let result = Command::new(executable).arg(&file).output().unwrap();
            child.wait().unwrap();
            std::fs::write(
                output.join(format!("{mode}.{}.out", outputs.len())),
                &result.stdout,
            )
            .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
            outputs.push(result.stdout);
        }
        assert_eq!(outputs[0], outputs[1], "{mode}");
    }
}
