//! Deep input must never overflow the stack: a stack overflow cannot be caught
//! and would abort the whole server process. Nesting beyond the parser's limit
//! is an error; long left-deep chains, which generated code does produce, parse
//! and drop fine.
//!
//! Every case runs on a deliberately small 2 MB stack, the default for threads
//! in Rust, to prove the caller's stack size does not matter.

use phera_parser::parse;

const SMALL_STACK: usize = 2 * 1024 * 1024;
const DEEP: usize = 100_000;

fn on_small_stack(source: String) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(move || {
            // The tree is dropped here too, on the small stack.
            parse(&source).map(drop).map_err(|error| error.message)
        })
        .expect("cannot spawn thread")
        .join()
        .expect("parser panicked")
}

fn nested(open: &str, close: &str, depth: usize) -> String {
    format!("<?php {}1{};", open.repeat(depth), close.repeat(depth))
}

fn assert_too_deep(name: &str, source: String) {
    match on_small_stack(source) {
        Err(message) if message.contains("too deep") => {}
        other => panic!("{name}: expected a nesting error, got {other:?}"),
    }
}

#[test]
fn nesting_beyond_the_limit_is_an_error() {
    let cases = [
        ("parentheses", nested("(", ")", DEEP)),
        ("arrays", nested("[", "]", DEEP)),
        ("calls", nested("f(", ")", DEEP)),
        ("not", nested("!", "", DEEP)),
        ("negation", nested("- ", "", DEEP)),
        ("casts", nested("(int)", "", DEEP)),
        ("error suppression", nested("@", "", DEEP)),
        ("assignments", nested("$a=", "", DEEP)),
        ("power", nested("1**", "", DEEP)),
        ("null coalescing", nested("1??", "", DEEP)),
        ("ternaries", nested("1?1:(", ")", DEEP)),
        ("arrow functions", nested("fn()=>", "", DEEP)),
        ("closures", nested("function(){return ", ";}", DEEP)),
        ("interpolation", nested("\"{$a[", "]}\"", DEEP)),
        ("match", nested("match(1){1=>", "}", DEEP)),
        ("new", nested("new (", ")", DEEP)),
        (
            "blocks",
            format!("<?php {}{}", "{".repeat(DEEP), "}".repeat(DEEP)),
        ),
        (
            "if statements",
            format!("<?php {}{}", "if(1){".repeat(DEEP), "}".repeat(DEEP)),
        ),
        (
            "elseif chain",
            format!("<?php if(1){{}}{}", "elseif(1){}".repeat(DEEP)),
        ),
        (
            "else if chain",
            format!("<?php if(1){{}}{}", "else if(1){}".repeat(DEEP)),
        ),
        (
            "while loops",
            format!("<?php {}{}", "while(1){".repeat(DEEP), "}".repeat(DEEP)),
        ),
        (
            "anonymous classes",
            format!(
                "<?php {}1;{}",
                "new class{function f(){".repeat(DEEP),
                "}};".repeat(DEEP)
            ),
        ),
    ];

    for (name, source) in cases {
        assert_too_deep(name, source);
    }
}

#[test]
fn realistic_nesting_is_fine() {
    for source in [
        nested("(", ")", 300),
        nested("[", "]", 300),
        nested("f(", ")", 300),
        format!("<?php {}{}", "if(1){".repeat(300), "}".repeat(300)),
        format!("<?php if(1){{}}{}", "elseif(1){}".repeat(900)),
    ] {
        assert_eq!(on_small_stack(source), Ok(()));
    }
}

#[test]
fn long_chains_parse_and_drop() {
    let cases = [
        ("addition", format!("<?php {}1;", "1+".repeat(DEEP))),
        (
            "concatenation",
            format!("<?php {}'a';", "'a'.".repeat(DEEP)),
        ),
        (
            "assigned sum",
            format!("<?php $total = {}1;", "1+".repeat(DEEP)),
        ),
        (
            "exit argument",
            format!("<?php exit({}1);", "1+".repeat(DEEP)),
        ),
        (
            "clone argument",
            format!("<?php clone({}1);", "1+".repeat(DEEP)),
        ),
        ("logical or", format!("<?php {}1;", "1||".repeat(DEEP))),
        ("pipe", format!("<?php 1{};", "|>f(...)".repeat(DEEP))),
        ("indexing", format!("<?php $a{};", "[1]".repeat(DEEP))),
        ("properties", format!("<?php $a{};", "->b".repeat(DEEP))),
        ("method calls", format!("<?php $a{};", "->b()".repeat(DEEP))),
        ("calls", format!("<?php f{};", "()".repeat(DEEP))),
        ("static calls", format!("<?php A{};", "::b()".repeat(DEEP))),
        ("statements", format!("<?php {}", "$a=1;".repeat(DEEP))),
        ("array items", format!("<?php $a=[{}];", "1,".repeat(DEEP))),
    ];

    for (name, source) in cases {
        assert_eq!(on_small_stack(source), Ok(()), "{name}");
    }
}
