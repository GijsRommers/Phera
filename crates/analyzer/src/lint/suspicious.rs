//! Detect suspicious syntax such as `if ($state = 1) {}` without inferring types.

mod blocks;
mod expressions;
mod literals;

use super::rule::*;
use super::{Context, traversal::Node};

pub(super) const RULES: &[Rule] = &[
    suspicious(
        "duplicate-array-key",
        "Detect literal array keys that overwrite an earlier item.",
        "$a = [1 => 'a', '1' => 'b'];",
        "$a = [1 => 'a', 2 => 'b'];",
        DefaultStatus::Enabled,
        Visit::Expression,
        literals::duplicate_keys,
    ),
    suspicious(
        "duplicate-match-arm",
        "Detect repeated literal conditions in a match.",
        "$a = match ($n) { 1 => 'a', 1 => 'b' };",
        "$a = match ($n) { 1 => 'a', 2 => 'b' };",
        DefaultStatus::Enabled,
        Visit::Expression,
        literals::duplicate_match,
    ),
    suspicious(
        "duplicate-switch-case",
        "Detect repeated literal cases in a switch.",
        "switch ($n) { case 1: break; case 1: break; }",
        "switch ($n) { case 1: break; case 2: break; }",
        DefaultStatus::Enabled,
        Visit::Statement,
        literals::duplicate_cases,
    ),
    suspicious(
        "assignment-in-condition",
        "Detect simple assignments used as conditions.",
        "if ($count = 1) { echo $count; }",
        "if ($count === 1) { echo $count; }",
        DefaultStatus::Enabled,
        Visit::Statement,
        blocks::assignment_in_condition,
    ),
    suspicious(
        "constant-condition",
        "Detect constant conditions in conditional statements.",
        "if (false) { start(); }",
        "if ($enabled) { start(); }",
        DefaultStatus::OptIn,
        Visit::Statement,
        blocks::constant_condition,
    ),
    suspicious(
        "identical-branches",
        "Detect if and else branches with identical source.",
        "if ($ok) { run(); } else { run(); }",
        "run();",
        DefaultStatus::Enabled,
        Visit::Statement,
        blocks::identical_branches,
    ),
    suspicious(
        "loop-runs-once",
        "Detect a loop whose body exits unconditionally on the first iteration.",
        "foreach ($items as $item) { useItem($item); break; }",
        "if (isset($items[0])) { useItem($items[0]); }",
        DefaultStatus::OptIn,
        Visit::Statement,
        blocks::single_iteration,
    ),
    suspicious(
        "unreachable-statement",
        "Detect a statement immediately after an unconditional exit in the same block.",
        "function f() { return 1; echo 2; }",
        "function f() { return 1; }",
        DefaultStatus::Enabled,
        Visit::Block,
        blocks::after_exit,
    ),
    suspicious(
        "switch-fallthrough",
        "Require a comment or an explicit exit between nonempty cases.",
        "switch ($n) { case 1: a(); case 2: b(); }",
        "switch ($n) { case 1: a(); break; case 2: b(); }",
        DefaultStatus::Enabled,
        Visit::Statement,
        blocks::fallthrough,
    ),
    suspicious(
        "empty-catch",
        "Detect a catch handler that silently discards an exception.",
        "try { run(); } catch (\\Exception) {}",
        "try { run(); } catch (\\Exception $e) { logError($e); }",
        DefaultStatus::OptIn,
        Visit::Statement,
        blocks::empty_catch,
    ),
    suspicious(
        "rethrow-only-catch",
        "Detect a final catch handler that only rethrows its exception.",
        "try { run(); } catch (\\Exception $e) { throw $e; }",
        "run();",
        DefaultStatus::Enabled,
        Visit::Statement,
        blocks::rethrow,
    ),
    suspicious(
        "return-in-finally",
        "Detect return statements in a finally block.",
        "try { run(); } finally { return; }",
        "try { run(); } finally { cleanUp(); }",
        DefaultStatus::Enabled,
        Visit::Statement,
        blocks::return_in_finally,
    ),
    suspicious(
        "unreachable-catch",
        "Detect catch types already covered by an earlier handler.",
        "try { run(); } catch (\\Throwable) {} catch (\\Exception) {}",
        "try { run(); } catch (\\Exception) {} catch (\\Throwable) {}",
        DefaultStatus::OptIn,
        Visit::Statement,
        blocks::unreachable_catch,
    ),
    Rule {
        name: "foreach-reference-not-unset",
        description: "Unbind a foreach reference before reusing its variable.",
        bad: "foreach ($items as &$item) { $item++; } $item = 0;",
        good: "foreach ($items as &$item) { $item++; } unset($item); $item = 0;",
        default: DefaultStatus::Enabled,
        group: Group::Suspicious,
        visit: Visit::Namespace,
        check: None,
        fix: "The default looks for a later use in the same callable along a block continuation. Explicit selection also reports references left in scope without reuse.",
    },
    suspicious(
        "discarded-exception",
        "Detect a broad exception catch that does not retain the exception.",
        "try { run(); } catch (\\Exception) { recover(); }",
        "try { run(); } catch (\\Exception $e) { recover($e); }",
        DefaultStatus::OptIn,
        Visit::Statement,
        blocks::discard_exception,
    ),
    suspicious(
        "goto",
        "Detect goto statements.",
        "goto ready; ready: run();",
        "run();",
        DefaultStatus::OptIn,
        Visit::Statement,
        blocks::goto,
    ),
    suspicious(
        "self-assignment",
        "Detect assignments of a variable to itself.",
        "$ready = $ready;",
        "$ready = true;",
        DefaultStatus::Enabled,
        Visit::Expression,
        expressions::self_assignment,
    ),
    suspicious(
        "self-comparison",
        "Detect comparisons of a variable with itself, allowing NaN checks.",
        "$same = $a === $a;",
        "$same = $a === $b;",
        DefaultStatus::Enabled,
        Visit::Expression,
        expressions::self_comparison,
    ),
    suspicious(
        "loose-comparison",
        "Detect loose comparisons between incompatible literals. Selecting the rule or group also checks dynamic null comparisons.",
        "$same = 0 == 'x';",
        "$same = 0 === 'x';",
        DefaultStatus::Enabled,
        Visit::Expression,
        expressions::loose_comparison,
    ),
    suspicious(
        "non-strict-array-search",
        "Require the strict flag on in_array() and array_search().",
        "$found = in_array($value, $values);",
        "$found = in_array($value, $values, true);",
        DefaultStatus::OptIn,
        Visit::Expression,
        expressions::array_search,
    ),
    suspicious(
        "empty-on-literal",
        "Detect empty() on a literal whose truthiness is already known.",
        "$empty = empty('');",
        "$empty = true;",
        DefaultStatus::Enabled,
        Visit::Expression,
        expressions::empty_literal,
    ),
    suspicious(
        "unused-expression",
        "Detect standalone literals that have no effect.",
        "function f() { 123; }",
        "function f() { return 123; }",
        DefaultStatus::Enabled,
        Visit::Statement,
        blocks::unused_expression,
    ),
    suspicious(
        "variable-variables",
        "Flag variables whose names are computed at runtime.",
        "echo ${$name};",
        "echo $values[$name];",
        DefaultStatus::OptIn,
        Visit::Expression,
        expressions::dynamic_variable,
    ),
    suspicious(
        "extract",
        "Detect calls that create local bindings from an array with extract().",
        "extract($data);",
        "$name = $data['name'];",
        DefaultStatus::OptIn,
        Visit::Expression,
        expressions::extract,
    ),
    suspicious(
        "eval",
        "Detect dynamic evaluation of PHP source.",
        "eval($expression);",
        "$handlers[$name]();",
        DefaultStatus::OptIn,
        Visit::Expression,
        expressions::eval,
    ),
    suspicious(
        "debug-call",
        "Detect calls commonly left behind after debugging.",
        "var_dump($result);",
        "return $result;",
        DefaultStatus::OptIn,
        Visit::Expression,
        expressions::debug,
    ),
    suspicious(
        "error-suppression",
        "Detect the PHP error-control operator.",
        "@unlink($file);",
        "if (is_file($file)) { unlink($file); }",
        DefaultStatus::OptIn,
        Visit::Expression,
        expressions::error_suppression,
    ),
];

const fn suspicious(
    name: &'static str,
    description: &'static str,
    bad: &'static str,
    good: &'static str,
    default: DefaultStatus,
    visit: Visit,
    check: Check,
) -> Rule {
    Rule {
        name,
        description,
        bad,
        good,
        default,
        visit,
        check: Some(check),
        fix: "",
        group: Group::Suspicious,
    }
}

fn conditions(node: Node<'_>, mut visit: impl FnMut(&phera_ast::Expression)) {
    use phera_ast::StatementKind::*;
    let Node::Statement(statement) = node else {
        return;
    };
    match &statement.kind {
        If {
            condition, elseifs, ..
        } => {
            visit(condition);
            for branch in elseifs {
                visit(&branch.condition);
            }
        }
        While { condition, .. } | DoWhile { condition, .. } => visit(condition),
        For { condition, .. } => condition.last().into_iter().for_each(visit),
        _ => {}
    }
}

pub(super) fn variable(expression: &phera_ast::Expression) -> Option<&str> {
    use phera_ast::{ExpressionKind, Variable};
    match &expression.kind {
        ExpressionKind::Variable(Variable::Named(name)) => Some(name),
        _ => None,
    }
}
