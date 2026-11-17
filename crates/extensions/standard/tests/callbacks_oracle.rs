//! Callback values, mutations, diagnostics and deterministic callback traces against PHP 8.5.

mod callback_support;

mod callback_matrix;

use callback_matrix::Matrix;
use callback_support::list;
use phera_extension_api::{ArrayKey, PhpArray, Value};

const FUNCTIONS: [&str; 21] = [
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
];

fn arguments(name: &str, array: Value, other: Value, third: Option<Value>) -> Vec<Value> {
    if name == "array_filter" {
        return vec![array, Value::from("probe_truth")];
    }
    if name.starts_with("array_walk") {
        return vec![array, Value::from("probe_walk"), Value::from("extra")];
    }
    if matches!(
        name,
        "array_find" | "array_find_key" | "array_any" | "array_all"
    ) {
        return vec![array, Value::from("probe_truth")];
    }
    if name == "array_reduce" {
        return vec![array, Value::from("max"), Value::Int(-99)];
    }
    if name.ends_with("sort") {
        return vec![array, Value::from("probe_compare")];
    }
    let mut result = vec![array, other];
    result.extend(third);
    result.push(Value::from("probe_compare"));
    if matches!(name, "array_udiff_uassoc" | "array_uintersect_uassoc") {
        result.push(Value::from("probe_compare"));
    }
    result
}

fn sample(seed: usize) -> Value {
    let values = [
        Value::Int(0),
        Value::Int(2),
        Value::Int(-2),
        Value::from("3"),
        Value::Bool(false),
        Value::Null,
        Value::from("abc"),
        Value::Float(2.5),
    ];
    let array = (0..seed % 29)
        .map(|index| {
            let key = match seed % 3 {
                0 => ArrayKey::Int(index as i64),
                1 => ArrayKey::str(format!("k{}", (index * 7) % 31)),
                _ if index % 2 == 0 => ArrayKey::Int(index as i64 - 5),
                _ => ArrayKey::str(format!("s{index}")),
            };
            (key, values[(index * 7 + seed) % values.len()].clone())
        })
        .collect::<PhpArray>();
    Value::from(array)
}

#[test]
fn callback_matrix_matches_php() {
    let mut matrix = Matrix::new();
    for seed in 0..90 {
        for name in FUNCTIONS {
            matrix.check(
                name,
                arguments(
                    name,
                    sample(seed),
                    sample(seed + 5),
                    Some(sample(seed + 13)),
                ),
                None,
                None,
                false,
            );
        }
    }
    for name in FUNCTIONS {
        let large = list((0..10_000).rev().map(Value::Int));
        matrix.check(
            name,
            arguments(name, large, sample(17), None),
            None,
            None,
            false,
        );
    }
    matrix.finish("callbacks_matrix");
}

#[test]
fn callback_edges_match_php() {
    let mut matrix = Matrix::new();
    for name in FUNCTIONS {
        matrix.check(name, vec![], None, None, false);
        matrix.check(
            name,
            vec![Value::Int(4), Value::from("probe_compare")],
            None,
            None,
            false,
        );
        for callback in [Value::Null, Value::Int(42), list([]), Value::Bool(false)] {
            let mut args = arguments(name, sample(8), sample(4), None);
            let last = if name.contains("diff") || name.contains("intersect") {
                args.len() - 1
            } else {
                1
            };
            args[last] = callback;
            matrix.check(name, args, None, None, false);
        }
        matrix.check(
            name,
            arguments(name, sample(12), sample(18), None),
            None,
            Some(3),
            false,
        );
    }
    for name in ["usort", "uasort", "uksort"] {
        for result in [
            Value::Int(-1),
            Value::Int(0),
            Value::Int(1),
            Value::Bool(false),
            Value::Bool(true),
            Value::Null,
            Value::Float(0.9),
            Value::from("-2x"),
            list([]),
            list([Value::Int(1)]),
        ] {
            for length in [0, 1, 3, 8, 17, 33, 1025] {
                matrix.check(
                    name,
                    vec![
                        list((0..length).rev().map(Value::Int)),
                        Value::from("probe_compare"),
                    ],
                    Some(result.clone()),
                    None,
                    false,
                );
            }
        }
        for throw_at in [1, 2, 5, 13, 30] {
            matrix.check(
                name,
                arguments(name, sample(27), sample(0), None),
                None,
                Some(throw_at),
                false,
            );
        }
        matrix.check(
            name,
            arguments(name, sample(19), sample(0), None),
            None,
            None,
            true,
        );
    }
    for mode in [-1, 0, 1, 2, 3] {
        matrix.check(
            "array_filter",
            vec![sample(19), Value::from("probe_truth"), Value::Int(mode)],
            None,
            None,
            false,
        );
        matrix.check(
            "array_filter",
            vec![sample(19), Value::Null, Value::Int(mode)],
            None,
            None,
            false,
        );
    }
    for name in [
        "array_walk",
        "array_walk_recursive",
        "array_filter",
        "array_find",
        "array_reduce",
    ] {
        let nested = list([sample(8), list([sample(12)]), Value::Int(3)]);
        matrix.check(
            name,
            arguments(name, nested, sample(0), None),
            None,
            None,
            false,
        );
    }
    matrix.finish("callback_edges");
}

#[test]
fn set_callback_conversions_and_argument_errors_match_php() {
    let mut matrix = Matrix::new();
    for name in FUNCTIONS
        .into_iter()
        .filter(|name| name.contains("diff") || name.contains("intersect"))
    {
        for result in [
            Value::Bool(false),
            Value::Bool(true),
            Value::Null,
            Value::Float(0.9),
            Value::Float(f64::INFINITY),
            Value::Float(f64::NAN),
            Value::Float(1e20),
            Value::from("-2x"),
            list([]),
            list([Value::Int(1)]),
        ] {
            for length in [0, 3, 17] {
                matrix.check(
                    name,
                    arguments(
                        name,
                        sample(length),
                        sample(length + 3),
                        Some(sample(length + 5)),
                    ),
                    Some(result.clone()),
                    None,
                    false,
                );
            }
        }
        for values in [
            vec![],
            vec![sample(4)],
            vec![Value::Int(4), Value::Int(42)],
            vec![sample(4), Value::Int(42), Value::from("probe_compare")],
            vec![sample(4), Value::from("probe_compare")],
            vec![
                sample(4),
                Value::from("probe_compare"),
                Value::from("probe_compare"),
            ],
        ] {
            matrix.check(name, values, None, None, false);
        }
    }
    matrix.finish("callback_set_edges");
}

#[test]
fn three_way_associative_callbacks_match_php() {
    let mut matrix = Matrix::new();
    let array = |entries: &[(&str, &str)]| {
        Value::from(
            entries
                .iter()
                .map(|(key, value)| (ArrayKey::str(*key), Value::from(*value)))
                .collect::<PhpArray>(),
        )
    };
    for name in ["array_udiff_uassoc", "array_uintersect_uassoc"] {
        for (data, keys) in [
            ("probe_string", "probe_natural"),
            ("probe_natural", "probe_string"),
        ] {
            for first in [
                array(&[("x10", "a")]),
                array(&[("x10", "a"), ("x2", "b")]),
                array(&[("x2", "a"), ("x10", "b"), ("x1", "a")]),
            ] {
                for fourth in [false, true] {
                    let mut args = vec![
                        first.clone(),
                        first.clone(),
                        array(&[("x2", "a"), ("x10", "a"), ("x1", "b")]),
                    ];
                    if fourth {
                        args.push(array(&[("x1", "a"), ("x10", "a"), ("x2", "b")]));
                    }
                    args.extend([Value::from(data), Value::from(keys)]);
                    matrix.check(name, args, None, None, false);
                }
            }
        }
    }
    matrix.finish("callback_associative_three_way");
}
