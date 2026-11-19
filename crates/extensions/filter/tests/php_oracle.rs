//! Generated PHP 8.5 differential coverage for the filter extension.

use phera_ext_filter::Filter;
use phera_extension_api::{ClassEntry, Registry, Value, testing::oracle};

mod support;
use support::{Matrix, associative, inputs, list};

const REQUIRE_ARRAY: i64 = 1 << 24;
const REQUIRE_SCALAR: i64 = 1 << 25;
const FORCE_ARRAY: i64 = 1 << 26;
const NULL_ON_FAILURE: i64 = 1 << 27;
const THROW: i64 = 1 << 28;

#[test]
fn php_filter_matrix() {
    let mut matrix = Matrix::default();
    let shared = inputs::shared();
    let network = inputs::addresses();
    let filters: &[(&str, &[i64], bool)] = &[
        ("FILTER_VALIDATE_INT", &[1, 2, 3], false),
        ("FILTER_VALIDATE_BOOLEAN", &[], false),
        ("FILTER_VALIDATE_FLOAT", &[8192], false),
        ("FILTER_VALIDATE_REGEXP", &[], false),
        ("FILTER_VALIDATE_DOMAIN", &[1 << 20], true),
        ("FILTER_VALIDATE_URL", &[1 << 18, 1 << 19, 3 << 18], true),
        ("FILTER_VALIDATE_EMAIL", &[1 << 20], true),
        (
            "FILTER_VALIDATE_IP",
            &[1 << 20, 1 << 21, 3 << 20, 1 << 22, 1 << 23, 1 << 29],
            true,
        ),
        ("FILTER_VALIDATE_MAC", &[], true),
        (
            "FILTER_UNSAFE_RAW",
            &[4, 8, 16, 32, 64, 256, 512, 4 | 32 | 64],
            false,
        ),
        ("FILTER_SANITIZE_ENCODED", &[4, 8, 512, 4 | 8 | 512], false),
        (
            "FILTER_SANITIZE_SPECIAL_CHARS",
            &[4, 8, 32, 512, 4 | 32],
            false,
        ),
        ("FILTER_SANITIZE_FULL_SPECIAL_CHARS", &[128], false),
        ("FILTER_SANITIZE_EMAIL", &[], false),
        ("FILTER_SANITIZE_URL", &[], false),
        ("FILTER_SANITIZE_NUMBER_INT", &[], false),
        (
            "FILTER_SANITIZE_NUMBER_FLOAT",
            &[4096, 8192, 16384, 7 << 12],
            false,
        ),
        ("FILTER_SANITIZE_ADD_SLASHES", &[], false),
        ("FILTER_CALLBACK", &[], false),
    ];
    for &(filter, extra_flags, use_network) in filters {
        let mut inputs = shared.iter().collect::<Vec<_>>();
        if use_network {
            inputs.extend(&network);
        }
        for value in inputs {
            let value = oracle::php_literal(value);
            let data = match filter {
                "FILTER_CALLBACK" => "'options' => 'roundtrip',",
                "FILTER_VALIDATE_REGEXP" => "'options' => ['regexp' => '/^(?:[a-z]+|[0-9]+)$/i'],",
                _ => "",
            };
            if data.is_empty() {
                matrix.expression(format!("filter_var({value}, {filter})"));
            }
            for flags in [
                0,
                REQUIRE_ARRAY,
                FORCE_ARRAY,
                REQUIRE_SCALAR,
                NULL_ON_FAILURE,
                THROW,
                THROW | REQUIRE_ARRAY,
            ]
            .into_iter()
            .chain(extra_flags.iter().copied())
            {
                matrix.expression(format!(
                    "filter_var({value}, {filter}, [{data}'flags' => {flags}])"
                ));
            }
        }
    }
    matrix.run("filter-values", "");
}

#[test]
fn php_api_errors_and_metadata() {
    let mut matrix = Matrix::default();
    matrix.expression("filter_list()");
    let mut registry = Registry::for_level(phera_extension_api::testing::oracle::test_level());
    registry.class(ClassEntry::class("Exception"));
    registry.load(&Filter);
    let mut names = registry
        .constants()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    for name in names {
        matrix.expression(name);
    }
    let expected_names = oracle::run_php(
        "filter-constant-names",
        r#"<?php
        $constants = array_keys(get_defined_constants(true)['filter']);
        $names = array_filter($constants, fn($name) =>
            !(new ReflectionConstant($name))->isDeprecated());
        sort($names, SORT_STRING);
        echo implode("\n", $names);
    "#,
    );
    if let Some(expected_names) = expected_names {
        let mut names = registry
            .constants()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, expected_names);
    }
    for input in inputs::shared() {
        matrix.call("filter_id", &[input]);
    }
    matrix.expression("filter_var(filter: FILTER_VALIDATE_INT, value: '7')");
    matrix.expression("filter_var_array(add_empty: false, options: 257, array: ['1'])");
    for input in [
        "int",
        "int\0suffix",
        "Validate_IP",
        "validate_ip",
        "stripped",
        "callback",
    ] {
        matrix.call("filter_id", &[Value::from(input)]);
    }
    for expression in [
        "filter_id(new FilterName())",
        "filter_id(new stdClass())",
        "filter_list(0)",
        "filter_id()",
        "filter_id([], 1)",
        "filter_var()",
        "filter_var(1, 2, 3, 4)",
        "filter_var_array()",
        "filter_var_array(1)",
        "filter_var_array([], 1, 2, 3)",
        "filter_var(1, [])",
        "filter_var(1, null)",
        "filter_var(1, 257, null)",
        "filter_var(1, 257, new stdClass())",
        "filter_var(1, 257, 1.2)",
        "filter_var(1, 257, '1.2')",
        "filter_var(1, 257, '1e999')",
        "filter_var(1, 257, 'str')",
        "filter_var_array([], 'bad', [])",
        "filter_var_array([], 257, null)",
        "filter_var_array([], 257, [])",
        "filter_var_array([], null, null)",
        "filter_var('x', -1)",
        "filter_var('x', 256)",
        "filter_var('x', 512)",
        "filter_var('x', 521)",
        "filter_var('x', 278)",
        "filter_var('x', 524)",
        "filter_var('x', 516, ['filter' => 999])",
        "filter_var('42', 516, ['filter' => [], 'flags' => []])",
        "filter_var('42', 516, ['filter' => '257x', 'flags' => [0]])",
        "filter_var('42', 516, ['filter' => new stdClass(), 'flags' => new stdClass()])",
        "filter_var('42', 257, ['options' => ['min_range' => new stdClass()]])",
        "filter_var('42', 259, ['options' => ['min_range' => new stdClass()]])",
        "filter_var(new stdClass(), 257)",
        "filter_var(new stdClass(), 257, ['options' => ['default' => 'none']])",
        "filter_var(new stdClass(), 257, FILTER_THROW_ON_FAILURE)",
        "filter_var(new FilterString(), 257)",
        "filter_var('foo', FILTER_CALLBACK, ['options' => new FilterCallback()])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => ['FilterCallback', 'length']])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => 'FilterCallback::length'])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => fn($s) => [strlen($s), $s]])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => 'failure'])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => 'undefined_function'])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => ['FilterCallback', 'undefined']])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => ''])",
        "filter_var('foo', FILTER_CALLBACK, ['options' => 42])",
        "filter_var('foo', FILTER_CALLBACK)",
        "get_parent_class(Filter\\FilterException::class)",
        "get_parent_class(Filter\\FilterFailedException::class)",
        "(function () { try { filter_var('x', FILTER_VALIDATE_INT, FILTER_THROW_ON_FAILURE); }
            catch (Filter\\FilterException $e) { return $e->getMessage(); } })()",
        "(function () { preg_match('/./u', \"\\xff\"); filter_var('x', FILTER_VALIDATE_REGEXP,
             ['options' => ['regexp' => '/x/']]); return preg_last_error(); })()",
        "(function () { preg_match('/./u', \"\\xff\"); filter_var('x', FILTER_VALIDATE_REGEXP,
             ['options' => ['regexp' => '/(/']]); return preg_last_error(); })()",
    ] {
        matrix.expression(expression);
    }
    matrix.run("filter-api", "");
}

#[test]
fn php_filter_array_and_strict_calls() {
    let mut matrix = Matrix::default();
    for datum in inputs::shared() {
        let data = associative(&[("leaf", datum.clone()), ("nested", list(&[datum]))]);
        for id in [0, 257, 258, 259, 277, 516, 515, 522, 523, 999, 1024] {
            matrix.call("filter_var_array", &[data.clone(), Value::Int(id)]);
        }
        matrix.call(
            "filter_var_array",
            &[
                data.clone(),
                associative(&[
                    ("missing", Value::Int(257)),
                    ("leaf", Value::Int(257)),
                    (
                        "nested",
                        associative(&[
                            ("filter", Value::Int(258)),
                            ("flags", Value::Int(REQUIRE_ARRAY)),
                        ]),
                    ),
                ]),
            ],
        );
        matrix.call(
            "filter_var_array",
            &[
                data,
                associative(&[("missing", Value::Int(257)), ("leaf", Value::Int(999))]),
                Value::Bool(false),
            ],
        );
    }
    for input in [
        "filter_var_array(['x' => '1'], ['x' => 277, 'y' => 257])",
        "filter_var_array([], ['' => 257])",
        "filter_var_array([], [257])",
        "filter_var_array(['a' => 'x'], ['a' => ['filter' => 257, 'flags' => 402653184]])",
        "filter_var_array(['a' => 'x'], ['a' => ['filter' => 1024, 'options' => 'roundtrip']])",
        "filter_var_array(['a' => ['x']], ['a' => ['filter' => 1024, 'options' => 'roundtrip']])",
        "(function () { $a = '7'; $b = [&$a]; return [filter_var_array($b, 257), $a]; })()",
        "(function () { $a = ['7']; $b = [&$a]; return [filter_var_array($b, 257), $a]; })()",
    ] {
        matrix.expression(input);
    }
    matrix.run("filter-arrays", "");

    let mut strict = Matrix::default();
    for expression in [
        "filter_id(new FilterName())",
        "filter_id(null)",
        "filter_id(true)",
        "filter_var(1, '257')",
        "filter_var(1, 257, null)",
        "filter_var(1, 257, false)",
        "filter_var(1, 257, '123')",
        "filter_var_array([], true)",
        "filter_var_array([], 257, 1)",
        "filter_var_array([], 257, null)",
    ] {
        strict.expression(expression);
    }
    strict.run("filter-strict", "declare(strict_types=1);\n");
}
