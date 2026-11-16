//! Array and natural-order functions compared with the real PHP interpreter.
use phera_ext_standard::Standard;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};

#[test]
fn arrays_match_php() {
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    let mut check = |name: &str, arguments: Vec<Value>| {
        let expression = format!(
            "{name}({})",
            arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        );
        script.push_str(&format!("run('{name}', fn() => {expression});\n"));
        labels.push(expression);
        let mut context = TestContext::default();
        let result = call(&registry, &mut context, name, arguments);
        rust.push(oracle::encode_case(
            name,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    };
    let mut strings = vec![
        "".to_string(),
        " ".into(),
        "\t".into(),
        "\0".into(),
        "a\0b".into(),
    ];
    for prefix in ["", " ", "x", "X", "x0", "1.", "\t"] {
        for number in 0..25 {
            strings.push(format!("{prefix}{number}"));
        }
    }
    for name in ["strnatcmp", "strnatcasecmp"] {
        for left in &strings {
            for right in &strings {
                check(
                    name,
                    vec![Value::from(left.as_str()), Value::from(right.as_str())],
                );
            }
        }
    }
    let mut arrays = vec![
        Value::from(phera_extension_api::PhpArray::new()),
        Value::from(
            [
                (phera_extension_api::ArrayKey::Int(-3), Value::Int(1)),
                (phera_extension_api::ArrayKey::str("x"), Value::Null),
            ]
            .into_iter()
            .collect::<phera_extension_api::PhpArray>(),
        ),
        Value::from(
            [(
                phera_extension_api::ArrayKey::str("x"),
                Value::from(
                    [(phera_extension_api::ArrayKey::str("y"), Value::Int(3))]
                        .into_iter()
                        .collect::<phera_extension_api::PhpArray>(),
                ),
            )]
            .into_iter()
            .collect::<phera_extension_api::PhpArray>(),
        ),
    ];
    for scalar in [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(i64::MIN),
        Value::Int(i64::MAX),
        Value::Float(-0.0),
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::from("0"),
        Value::from("abc"),
    ] {
        arrays.push(Value::from(
            [(phera_extension_api::ArrayKey::str("x"), scalar)]
                .into_iter()
                .collect::<phera_extension_api::PhpArray>(),
        ));
    }
    arrays.push(Value::from(
        [
            Value::Null,
            Value::Bool(false),
            Value::Bool(true),
            Value::Int(0),
            Value::Float(-0.0),
            Value::from("0"),
            Value::from("abc"),
            Value::Float(f64::INFINITY),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, value)| (phera_extension_api::ArrayKey::Int(index as i64 - 3), value))
        .collect::<phera_extension_api::PhpArray>(),
    ));
    for name in [
        "array_merge",
        "array_replace",
        "array_merge_recursive",
        "array_replace_recursive",
    ] {
        for a in &arrays {
            for b in &arrays {
                for c in &arrays {
                    check(name, vec![a.clone(), b.clone(), c.clone()]);
                }
            }
        }
    }

    for array in &arrays {
        check("array_flip", vec![array.clone()]);
        check("array_fill_keys", vec![array.clone(), Value::Null]);
        for other in &arrays {
            check("array_combine", vec![array.clone(), other.clone()]);
        }
        for length in [-5, 0, 5] {
            check(
                "array_pad",
                vec![array.clone(), Value::Int(length), Value::Null],
            );
        }
    }
    for start in [-5, 0, 5, i64::MAX] {
        for count in [-1, 0, 1, 3, 2147483648] {
            check(
                "array_fill",
                vec![Value::Int(start), Value::Int(count), Value::Null],
            );
        }
    }
    let endpoints = vec![
        Value::Int(-3),
        Value::Int(0),
        Value::Int(5),
        Value::Float(0.5),
        Value::from("a"),
        Value::from("z"),
        Value::from("0"),
        Value::from("4"),
        Value::from("12"),
        Value::from(""),
        Value::from("abc"),
        Value::Null,
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::Float(9.9),
    ];
    for start in &endpoints {
        for end in &endpoints {
            for step in [-2.0, 0.0, 0.5, 1.0, 3.0] {
                check(
                    "range",
                    vec![start.clone(), end.clone(), Value::Float(step)],
                );
            }
        }
    }

    for array in &arrays {
        check("array_count_values", vec![array.clone()]);
        for key in [Value::Null, Value::Int(0), Value::from("x")] {
            check("array_column", vec![array.clone(), key]);
        }
        for length in [-1, 0, 1, 2, 10] {
            for preserve in [false, true] {
                check(
                    "array_chunk",
                    vec![array.clone(), Value::Int(length), Value::Bool(preserve)],
                );
            }
        }
        for flag in [0, 1, 2, 5, 6, 8, 10, 14, 99] {
            check("array_unique", vec![array.clone(), Value::Int(flag)]);
        }
        for other in &arrays {
            for name in [
                "array_diff",
                "array_diff_key",
                "array_diff_assoc",
                "array_intersect",
                "array_intersect_key",
                "array_intersect_assoc",
            ] {
                // PHP's intersect sorts temporary tables. Conversion-warning
                // counts depend on its internal comparison sequence, not the
                // returned set. Keep these diagnostic-sensitive pairs out.
                if name == "array_intersect"
                    && [array, other].iter().any(|value| match value {
                        Value::Array(array) => array.values().any(|value| {
                            value.is_array()
                                || matches!(value,Value::Float(number) if number.is_nan())
                        }),
                        _ => false,
                    })
                {
                    continue;
                }
                check(name, vec![array.clone(), other.clone()]);
            }
        }
    }
    for name in [
        "array_merge",
        "array_merge_recursive",
        "array_replace",
        "array_replace_recursive",
        "array_combine",
        "array_flip",
        "array_fill",
        "array_fill_keys",
        "array_pad",
        "range",
        "array_unique",
        "array_count_values",
        "array_column",
        "array_chunk",
        "array_diff",
        "array_diff_key",
        "array_diff_assoc",
        "array_intersect",
        "array_intersect_key",
        "array_intersect_assoc",
        "strnatcmp",
        "strnatcasecmp",
    ] {
        check(name, vec![]);
        check(name, vec![Value::Null]);
        check(name, vec![Value::Bool(false), Value::from("invalid")]);
    }
    for index in [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(-5),
        Value::Float(1.5),
        Value::Float(f64::NAN),
        Value::from("05"),
        Value::from("5"),
        Value::from(phera_extension_api::PhpArray::new()),
    ] {
        let row = Value::from(
            [
                (phera_extension_api::ArrayKey::str("v"), Value::Int(3)),
                (phera_extension_api::ArrayKey::str("k"), index),
            ]
            .into_iter()
            .collect::<phera_extension_api::PhpArray>(),
        );
        let rows = Value::from(
            [(phera_extension_api::ArrayKey::Int(0), row)]
                .into_iter()
                .collect::<phera_extension_api::PhpArray>(),
        );
        for column in [Value::from("v"), Value::Null, Value::from("missing")] {
            check("array_column", vec![rows.clone(), column, Value::from("k")]);
        }
    }
    for start in [-1.1, -0.3, -0.1, -0.0, 0.1, 0.2, 0.3, 0.7, 1.1] {
        for end in [-1.1, -0.3, -0.1, -0.0, 0.1, 0.2, 0.3, 0.7, 1.1] {
            for step in [0.1, 0.2, 0.3, 0.7, -0.1] {
                check(
                    "range",
                    vec![Value::Float(start), Value::Float(end), Value::Float(step)],
                );
            }
        }
    }
    for start in [
        Value::Null,
        Value::Bool(false),
        Value::from(phera_extension_api::PhpArray::new()),
    ] {
        for end in [
            Value::Null,
            Value::Int(1),
            Value::from(phera_extension_api::PhpArray::new()),
        ] {
            for step in [
                Value::Null,
                Value::Bool(false),
                Value::from("2"),
                Value::from("invalid"),
            ] {
                check("range", vec![start.clone(), end.clone(), step]);
            }
        }
    }
    let mut unique_seed = 0x19273_u64;
    for flags in [0, 1, 2, 5, 6, 8, 10, 14, 99] {
        for _ in 0..300 {
            let mut array = phera_extension_api::PhpArray::new();
            for index in 0..12 {
                unique_seed = unique_seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1);
                let value = match (unique_seed >> 24) % 9 {
                    0 => Value::Null,
                    1 => Value::Bool(false),
                    2 => Value::Bool(true),
                    3 => Value::Float(-0.0),
                    4 => Value::Float(f64::NAN),
                    5 => Value::Float(f64::INFINITY),
                    6 => Value::from("abc"),
                    7 => Value::from("0"),
                    _ => Value::Int(3),
                };
                array.insert(phera_extension_api::ArrayKey::Int(index - 4), value);
            }
            check("array_unique", vec![Value::from(array), Value::Int(flags)]);
        }
    }
    for start in [i64::MIN, i64::MAX] {
        for step in [
            Value::Int(i64::MAX),
            Value::Int(i64::MIN),
            Value::Float(f64::NAN),
            Value::Float(f64::INFINITY),
        ] {
            check("range", vec![Value::Int(start), Value::Int(0), step]);
        }
    }
    for arguments in [
        vec![Value::Int(0), Value::Int(2147483648)],
        vec![Value::Float(0.0), Value::Float(2147483648.0)],
        vec![Value::Int(i64::MIN), Value::Int(i64::MAX)],
    ] {
        check("range", arguments);
    }
    for name in [
        "array_push",
        "array_pop",
        "array_shift",
        "array_unshift",
        "array_splice",
    ] {
        for array in &arrays {
            let tails = if name == "array_splice" {
                [i64::MIN, -10, -1, 0, 1, 10, i64::MAX]
                    .into_iter()
                    .flat_map(|offset| {
                        [
                            Value::Null,
                            Value::Int(i64::MIN),
                            Value::Int(-1),
                            Value::Int(0),
                            Value::Int(2),
                            Value::Int(i64::MAX),
                        ]
                        .into_iter()
                        .flat_map(move |length| {
                            [Value::Null, Value::Bool(false), Value::from("abc")]
                                .into_iter()
                                .map(move |replacement| {
                                    vec![Value::Int(offset), length.clone(), replacement]
                                })
                        })
                    })
                    .collect()
            } else if name == "array_push" || name == "array_unshift" {
                vec![vec![], vec![Value::Null, Value::Int(7)]]
            } else {
                vec![vec![]]
            };
            for tail in tails {
                let expression = format!(
                    "{name}($v{})",
                    tail.iter()
                        .map(|v| format!(",{}", oracle::php_literal(v)))
                        .collect::<String>()
                );
                script.push_str(&format!(
                    "run('{name}',function() {{$v={};$r={expression};return [$r,$v];}});\n",
                    oracle::php_literal(array)
                ));
                labels.push(format!("{expression} on {}", oracle::php_literal(array)));
                let cell = std::rc::Rc::new(std::cell::RefCell::new(array.clone()));
                let mut arguments = vec![Value::Reference(cell.clone())];
                arguments.extend(tail);
                let mut context = TestContext::default();
                let result = call(&registry, &mut context, name, arguments).map(|value| {
                    Value::from(
                        [
                            (phera_extension_api::ArrayKey::Int(0), value),
                            (phera_extension_api::ArrayKey::Int(1), cell.borrow().clone()),
                        ]
                        .into_iter()
                        .collect::<phera_extension_api::PhpArray>(),
                    )
                });
                rust.push(oracle::encode_case(
                    name,
                    &result,
                    &context.output,
                    &context.diagnostics,
                ));
            }
        }
    }

    let mut seed = 71829_u64;
    for name in [
        "sort",
        "rsort",
        "asort",
        "arsort",
        "ksort",
        "krsort",
        "natsort",
        "natcasesort",
    ] {
        for flag in [0, 1, 2, 5, 6, 10, 14, 99] {
            for sample in 0..300 {
                let mut array = phera_extension_api::PhpArray::new();
                for index in 0..(if sample == 299 { 1030 } else { sample % 40 }) {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let number = ((seed >> 32) % 100) as i64;
                    array.insert(
                        if index % 2 == 0 {
                            phera_extension_api::ArrayKey::Int(index - 5)
                        } else {
                            phera_extension_api::ArrayKey::str(format!("k{index}"))
                        },
                        match (seed >> 24) % 12 {
                            0 => Value::Null,
                            1 => Value::Bool(false),
                            2 => Value::Bool(true),
                            3 => Value::Float(-0.0),
                            4 => Value::Float(f64::NAN),
                            5 => Value::Float(f64::INFINITY),
                            6 => Value::from("abc"),
                            7 => Value::from(""),
                            8 => Value::Float(number as f64 / 3.0),
                            9 => Value::from(number.to_string()),
                            _ => Value::Int(number),
                        },
                    );
                }
                let value = Value::from(array);
                let mut arguments = vec![];
                let tail = if name.starts_with("nat") {
                    String::new()
                } else {
                    arguments.push(Value::Int(flag));
                    format!(",{flag}")
                };
                let expression = format!("{name}($v{tail})");
                script.push_str(&format!(
                    "run('{name}',function() {{$v={};$r={expression};return [$r,$v];}});\n",
                    oracle::php_literal(&value)
                ));
                labels.push(format!("{expression} on {}", oracle::php_literal(&value)));
                let cell = std::rc::Rc::new(std::cell::RefCell::new(value));
                arguments.insert(0, Value::Reference(cell.clone()));
                let mut context = TestContext::default();
                let result = call(&registry, &mut context, name, arguments).map(|value| {
                    Value::from(
                        [
                            (phera_extension_api::ArrayKey::Int(0), value),
                            (phera_extension_api::ArrayKey::Int(1), cell.borrow().clone()),
                        ]
                        .into_iter()
                        .collect::<phera_extension_api::PhpArray>(),
                    )
                });
                rust.push(oracle::encode_case(
                    name,
                    &result,
                    &context.output,
                    &context.diagnostics,
                ));
            }
        }
    }
    // Recursion must fail without mutating the referenced input.
    for name in ["array_merge_recursive", "array_replace_recursive"] {
        script.push_str(&format!(
            "run('{name}',function() {{$a=[];$a['x']=&$a;return {name}($a,$a);}});\n"
        ));
        labels.push(format!("{name}($a,$a), $a['x']=&$a"));
        let cell = std::rc::Rc::new(std::cell::RefCell::new(Value::Null));
        let value = Value::from(
            [(
                phera_extension_api::ArrayKey::str("x"),
                Value::Reference(cell.clone()),
            )]
            .into_iter()
            .collect::<phera_extension_api::PhpArray>(),
        );
        *cell.borrow_mut() = value.clone();
        let mut context = TestContext::default();
        let result = call(&registry, &mut context, name, vec![value.clone(), value]);
        rust.push(oracle::encode_case(
            name,
            &result,
            &context.output,
            &context.diagnostics,
        ));
        *cell.borrow_mut() = Value::Null;
    }
    let Some(php) = oracle::run_php("standard-arrays", &script) else {
        panic!("PHP oracle required");
    };
    let mut counts = std::collections::BTreeMap::new();
    for label in &labels {
        if let Some((name, _)) = label.split_once('(') {
            *counts.entry(name).or_insert(0_usize) += 1;
        }
    }
    for (name, count) in counts {
        println!("{name}: {count} cases");
    }
    oracle::compare("arrays", &labels, &php, &rust);
}

/// The task's explicit depth limit must use a PHP Error rather than a Rust stack overflow.
#[test]
fn recursive_depth_guard() {
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    let mut value = Value::Null;
    for _ in 0..10_005 {
        value = Value::from(
            [(phera_extension_api::ArrayKey::str("nested"), value)]
                .into_iter()
                .collect::<phera_extension_api::PhpArray>(),
        );
    }
    for name in ["array_merge_recursive", "array_replace_recursive"] {
        let mut context = TestContext::default();
        let result = call(
            &registry,
            &mut context,
            name,
            vec![value.clone(), value.clone()],
        );
        assert!(
            matches!(result, Err(phera_extension_api::NativeError::Throw {class, message})
            if class == "Error" && message == "Maximum array recursion depth exceeded")
        );
    }
}
