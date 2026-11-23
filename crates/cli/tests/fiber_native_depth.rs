//! Fibers switch inside the callbacks of built-ins that run on the frame
//! stack, as in PHP, and the Rust-stack bound on awaited callbacks refuses a
//! switch without changing the fiber.

mod support;

use std::process::Command;

use support::Project;

fn run(script: &str) -> std::process::Output {
    let project = Project::new();
    let path = project.write("fiber-native-depth.php", script);
    Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn fibers_switch_inside_frame_stack_callbacks() {
    let output = run(r#"<?php
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
var_dump($parked->isTerminated());
"#);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(output.stdout, b"bool(true)\n");
}

#[test]
fn a_refused_switch_leaves_the_fiber_suspended() {
    // `array_map` over three arrays awaits its callback, which takes Rust
    // stack. Just under the callback limit the callbacks still run, and the
    // switch is the one refused.
    let output = run(r#"<?php
function down(int $depth, Closure $callback, mixed $_ = null) {
    if ($depth === 0) {
        $callback();
    } else {
        array_map('down', [$depth - 1], [$callback], [null]);
    }
}
$parked = new Fiber(fn () => Fiber::suspend('parked'));
echo $parked->start(), "\n";
try {
    down(63, fn () => $parked->resume('late'));
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
var_dump($parked->isSuspended());
$parked->resume('now');
var_dump($parked->isTerminated());
"#);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        output.stdout,
        b"parked\n\
          Cannot switch fibers in current execution context\n\
          bool(true)\n\
          bool(true)\n"
    );
}
