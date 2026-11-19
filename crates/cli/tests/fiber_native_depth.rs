//! A fiber suspended inside `array_map()` keeps those native callbacks alive.
//! Switching through another stack of callbacks must keep the worker alive.

mod support;

use std::process::Command;

use support::Project;

#[test]
fn fiber_switches_share_the_native_callback_limit() {
    let project = Project::new();
    let script = project.write(
        "fiber-native-depth.php",
        r#"<?php
function throughCallbacks(int $depth, Closure $callback) {
    if ($depth === 0) {
        $callback();
    } else {
        array_map(fn () => throughCallbacks($depth - 1, $callback), [null]);
    }
}
function nestedFibers(int $remaining) {
    if ($remaining > 0) {
        $fiber = new Fiber(fn () => throughCallbacks(22, fn () => nestedFibers($remaining - 1)));
        $fiber->start();
    }
}
try {
    nestedFibers(14);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
$parked = new Fiber(fn () => throughCallbacks(17, fn () => Fiber::suspend()));
$parked->start();
try {
    throughCallbacks(15, fn () => $parked->resume());
} catch (FiberError $error) {
    echo $error->getMessage(), "\n";
}
$parked->resume();
var_dump($parked->isTerminated());
"#,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg(script)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        output.stdout,
        b"Maximum native callback depth exceeded\n\
          Cannot switch fibers in current execution context\n\
          bool(true)\n"
    );
}
