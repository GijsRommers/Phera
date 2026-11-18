//! Turning `a=1&b[]=2&c[x][y]=3` into PHP arrays, the way PHP fills `$_GET`,
//! `$_POST` and `$_COOKIE`. The rules are those of `php_register_variable_ex`
//! in php-src's `main/php_variables.c`.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value, args,
};

pub(super) const ENTRY: FunctionEntry = FunctionEntry::sync(
    "parse_str",
    &[
        Param::new("string", ParamType::String),
        Param::new("result", ParamType::Mixed).by_ref(),
    ],
    parse_str,
);

fn parse_str(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let input = input.split(|&byte| byte == 0).next().unwrap_or_default();
    let mut result = PhpArray::new();
    if parse(&mut result, input, b'&', Duplicates::Replace) {
        context.warning(format!(
            "parse_str(): Input variables exceeded {MAX_INPUT_VARIABLES}. \
             To increase the limit change max_input_vars in php.ini."
        ));
    }
    if let Some(reference) = args::reference(arguments, 1) {
        *reference.borrow_mut() = Value::from(result);
    }
    Ok(Value::Null)
}

/// PHP's `max_input_vars`: a request cannot make the server build an
/// arbitrarily large array.
const MAX_INPUT_VARIABLES: usize = 1000;

/// PHP's `max_input_nesting_level`.
const MAX_NESTING: usize = 64;

/// What happens when a name occurs twice.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Duplicates {
    /// Query strings and forms: the last value wins.
    Replace,
    /// Cookies: the first value wins, because browsers send the most
    /// specific cookie first.
    KeepFirst,
}

/// Parses `name=value` pairs separated by `separator` into `variables`.
pub fn parse_pairs(variables: &mut PhpArray, input: &[u8], separator: u8, duplicates: Duplicates) {
    parse(variables, input, separator, duplicates);
}

fn parse(variables: &mut PhpArray, input: &[u8], separator: u8, duplicates: Duplicates) -> bool {
    let pairs = input
        .split(|&byte| byte == separator)
        .filter(|pair| !pair.is_empty());
    for (index, pair) in pairs.enumerate() {
        if index == MAX_INPUT_VARIABLES {
            return true;
        }
        let (name, value) = match pair.iter().position(|&byte| byte == b'=') {
            Some(equals) => (&pair[..equals], &pair[equals + 1..]),
            None => (pair, &[][..]),
        };
        let name = url_decode(name);
        let Some((keys, valid)) = parse_name(&name) else {
            continue;
        };
        if valid {
            store(
                variables,
                &name,
                &keys,
                Value::from(url_decode(value)),
                duplicates,
            );
        } else if let Some(Some(base)) = keys.first() {
            variables.remove(&array_key(base));
        }
    }
    false
}

/// Decodes `%41` to `A` and `+` to a space. A `%` that is not followed by two
/// hex digits stays as it is.
pub fn url_decode(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut rest = input;
    while let [first, tail @ ..] = rest {
        rest = tail;
        match (first, tail) {
            (b'+', _) => output.push(b' '),
            (b'%', [high, low, after @ ..]) => match (hex_digit(*high), hex_digit(*low)) {
                (Some(high), Some(low)) => {
                    output.push(high * 16 + low);
                    rest = after;
                }
                _ => output.push(b'%'),
            },
            _ => output.push(*first),
        }
    }
    output
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One step into the arrays: a named key, or `[]` meaning "append".
type Key = Option<Vec<u8>>;

/// Splits `user[address][]` into `user`, `address`, append. Returns `None`
/// for a name PHP ignores: an empty one, or one nested too deep.
fn parse_name(name: &[u8]) -> Option<(Vec<Key>, bool)> {
    // PHP treats names as C strings, so a NUL byte ends the name.
    let name = name.split(|&byte| byte == 0).next().unwrap_or_default();
    let name = &name[name.iter().take_while(|&&byte| byte == b' ').count()..];
    let bracket = name.iter().position(|&byte| byte == b'[');
    let (base, mut rest) = name.split_at(bracket.unwrap_or(name.len()));
    if base.is_empty() {
        return None;
    }
    // A PHP variable name cannot contain a space or a dot.
    let mut keys: Vec<Key> = vec![Some(base.iter().map(|&byte| sanitize(byte)).collect())];

    while let [b'[', inside @ ..] = rest {
        if keys.len() > MAX_NESTING {
            return Some((keys, false));
        }
        // PHP skips one whitespace byte when it looks for `[]`, but keeps
        // that byte as part of a named key.
        let probe = match inside {
            [first, tail @ ..] if is_c_space(*first) => tail,
            _ => inside,
        };
        let skipped = inside.len() - probe.len();
        if let [b']', after @ ..] = probe {
            keys.push(None);
            rest = after;
        } else if let Some(close) = probe.iter().position(|&byte| byte == b']') {
            keys.push(Some(inside[..skipped + close].to_vec()));
            rest = &probe[close + 1..];
        } else {
            // No closing bracket, so this was not an index after all. Right
            // after the variable name, PHP glues the rest onto the name. After
            // an index it drops the rest: a quirk of its in-place C string
            // handling that scripts may nonetheless depend on.
            if let [Some(base)] = keys.as_mut_slice() {
                base.push(b'_');
                base.extend(inside.iter().map(|&byte| match byte {
                    b'[' => b'_',
                    other => sanitize(other),
                }));
            }
            break;
        }
    }
    Some((keys, true))
}

fn sanitize(byte: u8) -> u8 {
    if matches!(byte, b' ' | b'.') {
        b'_'
    } else {
        byte
    }
}

fn is_c_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Stores `value` under the path of keys, creating arrays on the way and
/// replacing anything in the way that is not an array.
fn store(root: &mut PhpArray, name: &[u8], keys: &[Key], value: Value, duplicates: Duplicates) {
    let original = &name[name.iter().take_while(|&&byte| byte == b' ').count()..];
    let Some((last, containers)) = keys.split_last() else {
        return;
    };
    let mut current = root;
    for key in containers {
        if key.as_deref().is_some_and(|name| forbidden(name, original)) {
            return;
        }
        let Some(inner) = descend(current, key.as_deref()) else {
            return;
        };
        current = inner;
    }
    match last {
        Some(name) => {
            if forbidden(name, original) {
                return;
            }
            let key = array_key(name);
            if duplicates == Duplicates::KeepFirst && current.contains_key(&key) {
                return;
            }
            current.insert(key, value);
        }
        // Appending fails only when the largest integer key is taken.
        None => drop(current.push(value)),
    }
}

fn forbidden(name: &[u8], original: &[u8]) -> bool {
    [b"__Host-".as_slice(), b"__Secure-"]
        .iter()
        .any(|prefix| name.starts_with(prefix) && !original.starts_with(prefix))
}

/// Steps into the array stored under `key`, making one if needed.
fn descend<'a>(array: &'a mut PhpArray, key: Option<&[u8]>) -> Option<&'a mut PhpArray> {
    let key = match key {
        Some(name) => array_key(name),
        None => array.push(Value::from(PhpArray::new())).ok()?,
    };
    if !matches!(array.get(&key), Some(Value::Array(_))) {
        array.insert(key.clone(), Value::from(PhpArray::new()));
    }
    match array.get_mut(&key) {
        Some(Value::Array(inner)) => Some(Rc::make_mut(inner)),
        _ => None,
    }
}

/// `"5"` becomes the integer key 5, as everywhere in PHP.
fn array_key(name: &[u8]) -> ArrayKey {
    ArrayKey::from_value(&Value::from(name)).unwrap_or_else(|_| ArrayKey::str(name))
}

#[cfg(test)]
mod tests {
    use phera_extension_api::testing::oracle;

    use super::*;

    const QUERIES: &[&str] = &[
        "",
        "a=1",
        "a=1&b=2&a=3",
        "a",
        "a=",
        "=5",
        "&&a=1&&",
        "a=b=c",
        "a[]=1&a[]=2",
        "a[x]=1&a[y]=2",
        "a[x][y][z]=deep",
        "a[][x]=1&a[][x]=2",
        "a=1&a[x]=2",
        "a[x]=1&a=2",
        "a[5]=five&a[]=six&a[05]=text",
        "a[-1]=neg&a[]=next",
        "a.b=dot&a b=space&a+b=plus",
        "a%5Bx%5D=encoded",
        "a[x=open",
        "a[x][y=open",
        "a[]]=1",
        "a[x]y=tail",
        "a[x]y[z]=tail",
        "a[ ]=space",
        "a[ x]=space",
        "a[][=odd",
        " lead=1",
        "a[.]=dot&a[b.c]=dot",
        "[x]=nobase",
        "%20=blank",
        "a=%41%zz%4",
        "a=1+2%2B3",
        "caf%C3%A9=ok",
        "a%00b=nul&c=d%00e",
        "a[b][]=1&a[b][]=2&a[c]=3",
        "x[1][2][3][4][5]=n",
        "GLOBALS=1&this=2&_GET=3",
        "a[x]=1&a[x][y]=2",
        "a[%5D]=bracket",
        "a[[x]]=nested",
    ];

    #[test]
    fn query_strings_parse_like_php() {
        let mut script = String::from("<?php\n");
        script.push_str(oracle::PHP_PREAMBLE);
        let mut labels = Vec::new();
        let mut rust = Vec::new();
        for (index, query) in QUERIES.iter().enumerate() {
            let literal = oracle::php_literal(&Value::from(*query));
            script.push_str(&format!(
                "run('q{index}', function () {{ parse_str({literal}, $out); return $out; }});\n"
            ));
            labels.push(query.to_string());

            let mut variables = PhpArray::new();
            parse_pairs(&mut variables, query.as_bytes(), b'&', Duplicates::Replace);
            rust.push(format!(
                "q{index}={}",
                oracle::encode(&Value::from(variables))
            ));
        }
        let Some(php) = oracle::run_php("runtime-query", &script) else {
            eprintln!("php binary not found; skipping");
            return;
        };
        oracle::compare("query strings", &labels, &php, &rust);
    }

    #[test]
    fn nesting_deeper_than_php_allows_drops_the_variable() {
        let query = format!("a{}=1&b=2", "[x]".repeat(MAX_NESTING + 1));
        let mut variables = PhpArray::new();
        parse_pairs(&mut variables, query.as_bytes(), b'&', Duplicates::Replace);
        assert!(!variables.contains_key(&ArrayKey::str("a")));
        assert!(variables.contains_key(&ArrayKey::str("b")));
    }

    #[test]
    fn the_first_cookie_of_a_name_wins() {
        let mut cookies = PhpArray::new();
        parse_pairs(
            &mut cookies,
            b"id=first; id=second; theme=dark",
            b';',
            Duplicates::KeepFirst,
        );
        assert_eq!(
            cookies.get(&ArrayKey::str("id")),
            Some(&Value::from("first"))
        );
        assert_eq!(
            cookies.get(&ArrayKey::str("theme")),
            Some(&Value::from("dark"))
        );
    }
}
