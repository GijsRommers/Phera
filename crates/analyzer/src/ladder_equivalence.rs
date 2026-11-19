//! An `if` ladder and its nested form must give the same findings.
//!
//! ```php
//! if ($a) { A(); } elseif ($b) { B(); } else { C(); }
//! if ($a) { A(); } else { if ($b) { B(); } else { C(); } }
//! ```
//!
//! The analyzer walks a ladder rung by rung (`analysis/if_ladder.rs`) and
//! must reach the result the nested form reaches by recursion. Each checker
//! corpus fixture with an `if` is turned into ladders: `if (c) {A} else {B}`
//! becomes `if (c) {A} elseif (!c) {B} else {B}`, and an `if` without `else`
//! gains `elseif (c) {A}`. Then the ladders are written nested, and both
//! programs are analysed. The trees share their spans, so even the reported
//! lines must agree.

use std::path::{Path, PathBuf};

use phera_ast::*;

use crate::config::Options;
use crate::diagnostic::Diagnostic;
use crate::project::ParsedFile;

/// Every list of statements in `program`, outermost first, each visited once:
/// what `visit` does to a statement is seen when its own blocks are visited.
fn for_each_statement(program: &mut Program, visit: &mut impl FnMut(&mut Statement)) {
    let mut pending: Vec<&mut Vec<Statement>> = vec![&mut program.statements];
    while let Some(block) = pending.pop() {
        for statement in block.iter_mut() {
            visit(statement);
            match &mut statement.kind {
                StatementKind::If {
                    then,
                    elseifs,
                    otherwise,
                    ..
                } => {
                    pending.push(then);
                    pending.extend(elseifs.iter_mut().map(|elseif| &mut elseif.body));
                    pending.extend(otherwise.as_mut());
                }
                StatementKind::Block(body)
                | StatementKind::While { body, .. }
                | StatementKind::DoWhile { body, .. }
                | StatementKind::For { body, .. }
                | StatementKind::Foreach { body, .. } => pending.push(body),
                StatementKind::Declare { body, .. } => pending.extend(body.as_mut()),
                StatementKind::Namespace { statements, .. } => pending.push(statements),
                StatementKind::Function(function) => pending.push(&mut function.body),
                StatementKind::Class(class) => {
                    for member in &mut class.members {
                        if let ClassMemberKind::Method(method) = &mut member.kind {
                            pending.extend(method.body.as_mut());
                        }
                    }
                }
                StatementKind::Switch { cases, .. } => {
                    pending.extend(cases.iter_mut().map(|case| &mut case.body));
                }
                StatementKind::Try {
                    body,
                    catches,
                    finally,
                } => {
                    pending.push(body);
                    pending.extend(catches.iter_mut().map(|catch| &mut catch.body));
                    pending.extend(finally.as_mut());
                }
                _ => {}
            }
        }
    }
}

/// Turns every plain `if` into a two-rung ladder; returns how many.
fn into_ladders(program: &mut Program) -> usize {
    let mut ladders = 0;
    for_each_statement(program, &mut |statement| {
        let StatementKind::If {
            condition,
            then,
            elseifs,
            otherwise,
        } = &mut statement.kind
        else {
            return;
        };
        if !elseifs.is_empty() {
            return;
        }
        let (condition, body) = match otherwise {
            Some(body) => (
                Expression {
                    span: condition.span,
                    kind: ExpressionKind::Unary {
                        operator: UnaryOperator::Not,
                        operand: Box::new(condition.clone()),
                    },
                },
                body.clone(),
            ),
            None => (condition.clone(), then.clone()),
        };
        elseifs.push(ElseIf {
            span: condition.span,
            condition,
            body,
        });
        ladders += 1;
    });
    ladders
}

/// Writes every ladder as `if`s nested in `else` blocks.
fn into_nested(program: &mut Program) {
    for_each_statement(program, &mut |statement| {
        let StatementKind::If {
            elseifs, otherwise, ..
        } = &mut statement.kind
        else {
            return;
        };
        let mut tail = otherwise.take();
        for elseif in std::mem::take(elseifs).into_iter().rev() {
            tail = Some(vec![Statement {
                span: elseif.span,
                kind: StatementKind::If {
                    condition: elseif.condition,
                    then: elseif.body,
                    elseifs: Vec::new(),
                    otherwise: tail,
                },
            }]);
        }
        *otherwise = tail;
    });
}

fn findings(file: &ParsedFile, path: &Path, level: u8) -> Vec<Diagnostic> {
    let options = Options {
        level,
        paths: vec![path.display().to_string()],
        ..Options::default()
    };
    let (mut found, _) =
        crate::analyze_files(std::slice::from_ref(file), &[path.to_path_buf()], &options)
            .expect("analysis");
    found.sort();
    found
}

fn with_program(file: &ParsedFile, program: Program) -> ParsedFile {
    ParsedFile {
        path: file.path.clone(),
        source: file.source.clone(),
        lines: file.lines.clone(),
        program,
    }
}

/// Analyses `file` as ladders and nested, at both levels; panics on a difference.
fn assert_same_findings(file: &ParsedFile, path: &Path) -> usize {
    let mut ladder = file.program.clone();
    let ladders = into_ladders(&mut ladder);
    if ladders == 0 {
        return 0;
    }
    let mut nested = ladder.clone();
    into_nested(&mut nested);
    for level in [0, 1] {
        let as_ladder = findings(&with_program(file, ladder.clone()), path, level);
        let as_nested = findings(&with_program(file, nested.clone()), path, level);
        assert_eq!(as_ladder, as_nested, "{} at level {level}", path.display());
    }
    ladders
}

/// The analyzer runs on its own large stack, as `phera check` does.
fn on_analyzer_stack(test: impl FnOnce() + Send) {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, test)
            .expect("analyzer thread")
            .join()
            .expect("ladder and nested form differ");
    });
}

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<_> = std::fs::read_dir(root)
        .expect("corpus")
        .map(|entry| entry.expect("corpus entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "php"))
        .collect();
    files.sort();
    files
}

#[test]
fn corpus_conditions_give_the_same_findings_as_ladders_and_nested() {
    on_analyzer_stack(|| {
        let mut fixtures = 0;
        let mut ladders = 0;
        for path in corpus() {
            let Ok(Ok(file)) = crate::project::parse_file(&path) else {
                continue;
            };
            let made = assert_same_findings(&file, &path);
            if made > 0 {
                fixtures += 1;
                ladders += made;
            }
        }
        // Not vacuous: the corpus has dozens of conditions.
        assert!(fixtures >= 30, "{fixtures} fixtures with an if");
        assert!(ladders >= 40, "{ladders} ladders");
    });
}

#[test]
fn a_long_ladder_gives_the_same_findings_as_its_nested_form() {
    on_analyzer_stack(|| {
        // Below the analyzer's own depth limit, which the nested form reaches
        // by recursion and the ladder does not.
        let rungs: String = (0..200)
            .map(|rung| {
                format!("elseif ($v === {rung}) {{ $x{rung} = {rung}; echo $x{rung}, $y; }} ")
            })
            .collect();
        // `$y` is known to be set only through the first rung's "no"
        // outcome, so every later rung depends on being handed that scope.
        let source = format!(
            "<?php function f(int $v): int {{ if (rand(0, 1)) {{ $y = 1; }} \
             if (!isset($y)) {{ return 0; }} {rungs}\
             else {{ missing_in_else(); return $undefined; }} return $v; }}"
        );
        let directory = std::env::temp_dir().join(format!("phera-ladder-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("scratch directory");
        let path = directory.join("ladder.php");
        std::fs::write(&path, &source).expect("write");
        let file = crate::project::parse_file(&path)
            .expect("read")
            .expect("parse");
        let mut nested = file.program.clone();
        into_nested(&mut nested);
        for level in [0, 1] {
            let as_ladder = findings(&file, &path, level);
            let as_nested = findings(&with_program(&file, nested.clone()), &path, level);
            assert!(!as_ladder.is_empty(), "the else branch has findings");
            assert_eq!(as_ladder, as_nested, "level {level}");
        }
        let _ = std::fs::remove_dir_all(&directory);
    });
}
