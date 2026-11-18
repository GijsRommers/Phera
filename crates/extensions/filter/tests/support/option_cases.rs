//! Filter-specific options, errors and exception behavior.

use phera_extension_api::Value;

use super::{Matrix, inputs, options};
use crate::{FORCE_ARRAY, NULL_ON_FAILURE, REQUIRE_ARRAY, THROW};

#[test]
fn php_filter_options() {
    let mut matrix = Matrix::default();
    for input in inputs::shared() {
        for (filter, flag, fields) in [
            (
                257,
                3,
                vec![
                    ("min_range", Value::from("-12junk")),
                    ("max_range", Value::Int(100)),
                ],
            ),
            (258, 0, vec![("default", Value::from("fallback"))]),
            (
                258,
                NULL_ON_FAILURE,
                vec![("default", Value::from("fallback"))],
            ),
            (
                259,
                8192,
                vec![
                    ("decimal", Value::from(",")),
                    ("thousand", Value::from("#'")),
                    ("min_range", Value::Float(-42.5)),
                    ("max_range", Value::from("1250.5")),
                ],
            ),
            (259, 0, vec![("decimal", Value::from(".."))]),
            (259, 8192, vec![("thousand", Value::from(""))]),
            (
                259,
                0,
                vec![("decimal", Value::Bool(true)), ("thousand", Value::Int(1))],
            ),
            (272, 0, vec![]),
            (272, 0, vec![("regexp", Value::Int(12))]),
            (272, 0, vec![("regexp", Value::from("/\x01/i"))]),
            (272, 0, vec![("regexp", Value::from("/(?=.)/us"))]),
            (272, 0, vec![("regexp", Value::from("/("))]),
            (272, 0, vec![("regexp", Value::from("/(/"))]),
            (272, 0, vec![("regexp", Value::from("/a/z"))]),
            (276, 0, vec![("separator", Value::from(":"))]),
            (276, 0, vec![("separator", Value::from("::"))]),
        ] {
            matrix.call(
                "filter_var",
                &[input.clone(), Value::Int(filter), options(flag, &fields)],
            );
        }
        matrix.call(
            "filter_var",
            &[
                input.clone(),
                Value::Int(257),
                Value::Int(THROW | NULL_ON_FAILURE),
            ],
        );
        for flag in [
            0,
            NULL_ON_FAILURE,
            THROW,
            THROW | FORCE_ARRAY,
            THROW | REQUIRE_ARRAY,
        ] {
            matrix.call(
                "filter_var",
                &[input.clone(), Value::Int(257), Value::Int(flag)],
            );
        }
    }
    for input in inputs::addresses() {
        for parser in [
            "parse_url",
            "Uri\\Rfc3986\\Uri",
            "Uri\\WhatWg\\Url",
            "Invalid",
        ] {
            for flag in [0, 1 << 18, 1 << 19] {
                matrix.call(
                    "filter_var",
                    &[
                        input.clone(),
                        Value::Int(273),
                        options(flag, &[("uri_parser_class", Value::from(parser))]),
                    ],
                );
            }
        }
    }
    matrix.run("filter-options", "");
}
