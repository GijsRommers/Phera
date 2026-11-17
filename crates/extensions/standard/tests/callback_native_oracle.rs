//! Real native callbacks exercise argument conversion, exceptions and mixed values.

mod callback_support;

use std::cell::RefCell;
use std::rc::Rc;

use callback_support::{Harness, PHP_CALLBACKS, list, ready};
use phera_extension_api::testing::oracle;
use phera_extension_api::{ArrayKey, PhpArray, Value};

#[test]
fn native_callbacks_match_php() {
    let mut script = format!("<?php\n{}\n{PHP_CALLBACKS}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    let values = [
        Value::Int(3),
        Value::Int(-2),
        Value::Null,
        Value::from("A10"),
        Value::from("a2"),
        Value::from("0"),
        Value::Bool(false),
        list([Value::Int(1)]),
    ];
    for seed in 0..60 {
        let array = Value::from(
            (0..seed % 24)
                .map(|index| {
                    let key = if seed % 2 == 0 {
                        ArrayKey::Int(index as i64 - 4)
                    } else {
                        ArrayKey::str(format!("a{index}"))
                    };
                    (key, values[(seed + index * 3) % values.len()].clone())
                })
                .collect::<PhpArray>(),
        );
        for (name, callback) in [
            ("array_filter", "strlen"),
            ("array_filter", "is_int"),
            ("array_reduce", "intdiv"),
            ("array_reduce", "max"),
            ("array_walk", "strcmp"),
            ("array_walk_recursive", "max"),
            ("usort", "strcmp"),
            ("uasort", "strcasecmp"),
            ("uksort", "strnatcmp"),
            ("array_find", "strcmp"),
            ("array_find_key", "strcasecmp"),
            ("array_any", "strnatcmp"),
            ("array_all", "max"),
            ("array_udiff", "strcasecmp"),
            ("array_udiff_assoc", "strcmp"),
            ("array_udiff_uassoc", "strcmp"),
            ("array_diff_ukey", "strnatcmp"),
            ("array_diff_uassoc", "strcasecmp"),
            ("array_uintersect", "strcmp"),
            ("array_uintersect_assoc", "strcasecmp"),
            ("array_uintersect_uassoc", "strnatcmp"),
            ("array_intersect_ukey", "strcasecmp"),
            ("array_intersect_uassoc", "strcmp"),
        ] {
            let by_reference = name.ends_with("sort") || name.starts_with("array_walk");
            let target = Value::Reference(Rc::new(RefCell::new(array.clone())));
            let mut arguments = vec![if by_reference {
                target.clone()
            } else {
                array.clone()
            }];
            if name.contains("diff") || name.contains("intersect") {
                arguments.push(list(values.iter().rev().cloned()));
            }
            arguments.push(Value::from(callback));
            if name.ends_with("uassoc") && name.starts_with("array_u") {
                arguments.push(Value::from("strnatcmp"));
            }
            if name == "array_reduce" {
                arguments.push(Value::Int(11));
            }
            let mut source = arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>();
            if by_reference {
                source[0] = "$a".into();
            }
            let expression = format!("{name}({})", source.join(","));
            labels.push(expression.clone());
            script.push_str(&format!(
                "run('{name}', function() {{ $a={};\n\
                $r={expression}; return [$r,{}]; }});\n",
                oracle::php_literal(&array),
                if by_reference { "$a" } else { "null" }
            ));
            let mut context = Harness {
                trace_enabled: false,
                ..Harness::default()
            };
            let result = ready(context.invoke(name, arguments)).map(|result| {
                list([
                    result,
                    if by_reference {
                        target.deref().clone()
                    } else {
                        Value::Null
                    },
                ])
            });
            rust.push(oracle::encode_case(
                name,
                &result,
                &context.base.output,
                &context.base.diagnostics,
            ));
        }
    }
    let php = oracle::run_php("callback_native", &script).expect("PHP oracle required");
    oracle::compare("native callbacks", &labels, &php, &rust);
}

#[test]
fn references_inside_arrays_match_php() {
    let mut script = format!("<?php\n{}\n{PHP_CALLBACKS}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for name in [
        "array_filter",
        "array_reduce",
        "array_walk",
        "array_walk_recursive",
        "usort",
        "uasort",
        "uksort",
        "array_find",
        "array_find_key",
        "array_any",
        "array_all",
        "array_udiff",
        "array_udiff_assoc",
        "array_udiff_uassoc",
        "array_diff_ukey",
        "array_diff_uassoc",
        "array_uintersect",
        "array_uintersect_assoc",
        "array_uintersect_uassoc",
        "array_intersect_ukey",
        "array_intersect_uassoc",
    ] {
        let external = Rc::new(RefCell::new(Value::Int(7)));
        let array = list([
            Value::Reference(external.clone()),
            Value::Reference(external.clone()),
            Value::Int(1),
        ]);
        let target = Value::Reference(Rc::new(RefCell::new(array.clone())));
        let by_reference = name.ends_with("sort") || name.starts_with("array_walk");
        let callback = if name.starts_with("array_walk") {
            "probe_walk"
        } else if name == "array_filter"
            || name == "array_find"
            || name == "array_find_key"
            || name == "array_any"
            || name == "array_all"
        {
            "probe_truth"
        } else {
            "probe_compare"
        };
        let mut arguments = vec![if by_reference {
            target.clone()
        } else {
            array.clone()
        }];
        if name.contains("diff") || name.contains("intersect") {
            arguments.push(list([Value::Int(7)]));
        }
        arguments.push(Value::from(callback));
        if name.ends_with("uassoc") && name.starts_with("array_u") {
            arguments.push(Value::from(callback));
        }
        let mut source = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>();
        source[0] = "$a".into();
        let expression = format!("{name}({})", source.join(","));
        labels.push(expression.clone());
        script.push_str(&format!(
            "run('{name}', function() {{ $external=7;\n\
            $a=[&$external,&$external,1]; $r={expression}; return [$r,$a,$external]; }});\n"
        ));
        let mut context = Harness {
            trace_enabled: false,
            ..Harness::default()
        };
        let result = ready(context.invoke(name, arguments))
            .map(|result| list([result, target.deref().clone(), external.borrow().clone()]));
        rust.push(oracle::encode_case(
            name,
            &result,
            &context.base.output,
            &context.base.diagnostics,
        ));
    }
    let php = oracle::run_php("callback_references", &script).expect("PHP oracle required");
    oracle::compare("callback references", &labels, &php, &rust);
}

#[test]
fn callbacks_receive_existing_reference_cells() {
    let mut script = format!("<?php\n{}\n{PHP_CALLBACKS}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for name in [
        "array_filter",
        "array_reduce",
        "array_find",
        "array_find_key",
        "array_any",
        "array_all",
        "usort",
        "uasort",
        "array_udiff",
        "array_udiff_assoc",
        "array_udiff_uassoc",
        "array_uintersect",
        "array_uintersect_assoc",
        "array_uintersect_uassoc",
    ] {
        let external = Rc::new(RefCell::new(Value::Int(7)));
        let array = list([
            Value::Reference(external.clone()),
            Value::Int(2),
            Value::Int(1),
        ]);
        let target = Value::Reference(Rc::new(RefCell::new(array.clone())));
        let by_reference = name.ends_with("sort");
        let callback = if name == "array_reduce" {
            "probe_increment_second"
        } else {
            "probe_increment"
        };
        let mut arguments = vec![if by_reference {
            target.clone()
        } else {
            array.clone()
        }];
        if name.contains("diff") || name.contains("intersect") {
            arguments.push(list([Value::Int(7), Value::Int(2)]));
        }
        arguments.push(Value::from(callback));
        if name.ends_with("uassoc") {
            arguments.push(Value::from("probe_compare"));
        }
        let mut source = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>();
        source[0] = "$a".into();
        let expression = format!("{name}({})", source.join(","));
        labels.push(expression.clone());
        script.push_str(&format!(
            "run('{name}', function() {{ $external=7;\n\
            $a=[&$external,2,1]; $r={expression}; return [$r,$a,$external]; }});\n"
        ));
        let mut context = Harness {
            trace_enabled: false,
            ..Harness::default()
        };
        let result = ready(context.invoke(name, arguments))
            .map(|result| list([result, target.deref().clone(), external.borrow().clone()]));
        rust.push(oracle::encode_case(
            name,
            &result,
            &context.base.output,
            &context.base.diagnostics,
        ));
    }
    let php =
        oracle::run_php("callback_reference_parameters", &script).expect("PHP oracle required");
    oracle::compare("callback reference parameters", &labels, &php, &rust);
}
