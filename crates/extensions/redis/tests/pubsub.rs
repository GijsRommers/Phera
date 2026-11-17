//! Subscriptions with a PHP callback, compared with phpredis: a second, real
//! PHP process publishes while the subscriber runs, and the callback
//! unsubscribes after the first message.

use std::process::{Command, Stdio};

use phera_extension_api::testing::oracle::find_php;
use phera_extension_api::testing::scripts::{Comparison, Runtime};

#[test]
fn callbacks_and_unsubscribe_match_php() {
    let Some(comparison) = Comparison::from_environment("redis-pubsub") else {
        return;
    };
    for prefix in ["", "prefix:"] {
        for (mode, unsubscribe, publish) in [
            ("subscribe", "unsubscribe", "PUBLISH"),
            ("psubscribe", "punsubscribe", "PUBLISH"),
            ("ssubscribe", "sunsubscribe", "SPUBLISH"),
        ] {
            let channel = format!("phera:pubsub:{}:{mode}", std::process::id());
            let callback = if mode == "psubscribe" {
                "$r,$pattern,$channel,$message"
            } else {
                "$r,$channel,$message"
            };
            let script = comparison.artifacts.join(format!("{mode}.php"));
            std::fs::write(
                &script,
                format!(
                    r#"<?php
$r=new Redis();$r->connect('127.0.0.1',16399);
$r->setOption(Redis::OPT_PREFIX,'{prefix}');
var_dump($r->{mode}(['{channel}'],function({callback}) {{
var_dump($channel,$message);var_dump($r->{unsubscribe}(['{channel}']));
}}));
"#
                ),
            )
            .unwrap();
            let publisher = format!(
                r#"$r=new Redis();$r->connect('127.0.0.1',16399);
usleep(500000);for($i=0;$i<30;$i++){{$r->rawCommand('{publish}','{prefix}{channel}',"hello\0redis");
usleep(50000);}}"#
            );
            let [php, phera] = [Runtime::Php, Runtime::Phera].map(|runtime| {
                let mut publishing = Command::new(find_php().unwrap())
                    .args(["-n", "-r", &publisher])
                    .stdout(Stdio::null())
                    .spawn()
                    .unwrap();
                let output = comparison.command(runtime, &script).output().unwrap();
                publishing.wait().unwrap();
                output
            });
            comparison
                .check(&format!("{mode}-{}", prefix.len()), &php, &phera)
                .unwrap();
        }
    }
}
