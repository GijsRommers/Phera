//! Regression checks for expression exits, lvalue ordering and guarded declarations.
//! These programs are checked through the public CLI, without executing them.

mod check_support;
use check_support::Project;

#[test]
fn only_continuing_expression_paths_supply_variables_and_returns() {
    let project = Project::new();
    let cases = [
        "function f($flag): int { $flag ? throw new Exception : throw new Exception; }",
        "function f(): int { strlen(throw new Exception); }",
        "function f(): int { (throw new Exception)->run($missing); }",
        "function f(): int { new (throw new Exception); }",
        "function f(): int { [1, throw new Exception, $missing]; }",
        "function f($flag): int { match ($flag) { 0 => throw new Exception,
            default => throw new Exception }; }",
        "function f(): int { true && throw new Exception; }",
        "function f(): int { false || throw new Exception; }",
        "function f(): int { null ?? throw new Exception; }",
        "function f(): int { (throw new Exception) . $missing; }",
        "function f(): int { -(throw new Exception); }",
        "function f(): int { $a[throw new Exception] = 1; }",
        "if (rand(0, 1)) { $x = 1; } isset($x) || throw new Exception; echo $x;",
        "if (rand(0, 1)) { $x = 1; } $x ?? throw new Exception; echo $x;",
        "rand(0, 1) ? throw new Exception : ($x = 1); echo $x;",
        "[0 => $key, $key => $value] = [1, 2]; echo $key, $value;",
        "class A { function m($a) {} } class B { function m() {} }
            $x = new B; $x ??= new A; $x->m();",
        "$x = 1; while (isset($x)) { $y = 1; unset($x); } echo $y;",
        "class A { function m($a) {} } class B { function m() {} }
            $x = new A; $y =& $x; $z = new B; $y =& $z; $y = new B; $x->m(1);",
        "class A { function m($a) {} } class B { function m() {} }
            $x = new A; $f = function() use (&$x) { $x->m(); }; $x = new B; $f();",
        "class A { function m($a) {} } class B { function m() {} }
            $x = new A; $f = function() use (&$x) { $x = new B; }; $f(); $x->m();",
    ];
    for (index, body) in cases.iter().enumerate() {
        let path = format!("flow-{index}.php");
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
fn namespace_sensitive_guards_and_attributed_suppressions_are_honored() {
    let project = Project::new();
    project.write(
        "guards.php",
        r#"<?php
class C {}
if (defined('c::OPTIONAL')) { echo C::OPTIONAL; }
if (class_exists('Optional')) { function f(Optional $x) {} }
/** @psalm-suppress UndefinedFunction */
#[Deprecated]
function attributed() { missing(); }
"#,
    );
    let output = project.run(&["guards.php"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn void_returns_are_checked_even_in_unreachable_branches() {
    let project = Project::new();
    project.write(
        "void.php",
        "<?php function f(): void { if (false) { return 1; } }",
    );
    let output = project.run(&["void.php"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("return.void"));
}

#[test]
fn declarations_after_a_terminal_statement_still_have_their_bodies_checked() {
    let project = Project::new();
    project.write(
        "deferred.php",
        "<?php return; function deferred() { missing_inside_deferred(); }",
    );
    let output = project.run(&["deferred.php"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("function.notFound"));
}
