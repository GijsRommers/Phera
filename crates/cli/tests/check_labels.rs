//! `goto` and labels around `if` ladders, as `phera check` sees them.
//!
//! ```php
//! if ($v === 1) { goto out; } elseif ($v === 2) { return 2; } else { return 3; }
//! out: return 1;   // reachable through the goto
//! ```
//!
//! The analyzer walks a ladder rung by rung (see `analysis/if_ladder.rs`); a
//! branch whose condition is known is still analysed when it holds a label.

mod check_support;
use check_support::Project;

#[test]
fn labels_in_and_after_ladders_are_reachable() {
    let project = Project::new();
    let cases = [
        "function f($v): int { if ($v === 1) { goto out; } elseif ($v === 2) { return 2; }
            else { return 3; } out: return 1; } echo f(1);",
        "goto inside; if (false) { } elseif (false) { inside: $x = 1; } echo $x;",
        "goto inside; if (true) { $y = 1; } elseif (rand(0, 1)) { inside: $y = 2; } echo $y;",
        "$n = 0; again: if ($n === 0) { $n = 1; } elseif ($n === 1) { $n = 2; goto again; }
            echo $n;",
    ];
    for (index, body) in cases.iter().enumerate() {
        let path = format!("labels-{index}.php");
        project.write(&path, &format!("<?php {body}"));
        let output = project.run(&[&path]);
        assert!(
            output.status.success(),
            "{body}\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn code_after_a_ladder_is_still_checked() {
    let project = Project::new();
    project.write(
        "after.php",
        "<?php function g($v, $w) { if ($v) { return; } elseif ($w) { goto out; }
            out: missing_after_ladder(); }",
    );
    let output = project.run(&["after.php"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("function.notFound"));
}

#[test]
fn a_hundred_thousand_arm_ladder_of_gotos_is_checked_in_a_loop() {
    let project = Project::new();
    let arms: String = (0..100_000)
        .map(|arm| format!("elseif ($v === {arm}) {{ echo {arm}; goto done; }} "))
        .collect();
    project.write(
        "ladder.php",
        &format!("<?php $v = 99999; if ($v === -1) {{ goto done; }} {arms} done: echo 'done';"),
    );
    let output = project.run(&["ladder.php"]);
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
