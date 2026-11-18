//! E-mail validation uses PHP's two fixed PCRE patterns.
//!
//! ```php
//! filter_var('a@example.org', FILTER_VALIDATE_EMAIL); // 'a@example.org'
//! filter_var('é@example.org', FILTER_VALIDATE_EMAIL, FILTER_FLAG_EMAIL_UNICODE);
//! ```

use std::sync::LazyLock;

use pcre2::bytes::{Regex, RegexBuilder};

use crate::constants::EMAIL_UNICODE;
use crate::options::Flags;

pub fn valid(input: &[u8], flags: Flags) -> bool {
    if input.len() > 320 {
        return false;
    }
    let pattern = if flags.has(EMAIL_UNICODE) {
        &*UNICODE
    } else {
        &*ASCII
    };
    pattern.is_match(input).unwrap_or(false)
}

static ASCII: LazyLock<Regex> = LazyLock::new(|| compile(ASCII_PATTERN, false));
static UNICODE: LazyLock<Regex> = LazyLock::new(|| compile(UNICODE_PATTERN, true));

fn compile(pattern: &str, unicode: bool) -> Regex {
    RegexBuilder::new()
        .caseless(true)
        .utf(unicode)
        .ucp(unicode)
        .build(pattern)
        .expect("PHP's fixed e-mail pattern compiles")
}

// These patterns are derived from PHP 8.5.12's ext/filter/logical_filters.c.
// Copyright © Michael Rushton 2009-10, http://squiloople.com/
// The original author permits redistribution with this copyright notice.
const UNICODE_PATTERN: &str = concat!(
    r#"(*LIMIT_MATCH=1000000)(*LIMIT_DEPTH=100000)^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:"#,
    r#"\x22?[^\x5C\x22]\x22?)){255,})(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x2"#,
    r#"2]\x22?)){65,}@)(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E\pL\pN]+)"#,
    r#"|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F\pL\pN]|(?:\x5C[\x00-\x"#,
    r#"7F]))*\x22))(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E\pL\pN]+"#,
    r#")|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F\pL\pN]|(?:\x5C[\x00-\"#,
    r#"x7F]))*\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,"#,
    r#"126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\[(?:(?:I"#,
    r#"Pv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f"#,
    r#"0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?"#,
    r#":(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a"#,
    r#"-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))"#,
    r#"?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|("#,
    r#"?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\]))\z"#,
);
const ASCII_PATTERN: &str = concat!(
    r#"(*LIMIT_MATCH=1000000)(*LIMIT_DEPTH=100000)^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:"#,
    r#"\x22?[^\x5C\x22]\x22?)){255,})(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x2"#,
    r#"2]\x22?)){65,}@)(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x"#,
    r#"22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22))"#,
    r#"(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-"#,
    r#"\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22)))*@(?:(?:(?"#,
    r#"!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,126}){1,}(?:(?:[a-z][a-z"#,
    r#"0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}("#,
    r#"?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,"#,
    r#"4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{"#,
    r#"1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{"#,
    r#"1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4"#,
    r#"][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]"#,
    r#"{2})|(?:[1-9]?[0-9]))){3}))\]))\z"#,
);
