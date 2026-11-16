use phera_ext_standard::Standard;
use phera_extension_api::{
    testing::{TestContext, call},
    *,
};
use std::{fmt::Write, process::Command};
fn hex(s: &[u8]) -> String {
    s.iter().map(|b| format!("{b:02x}")).collect()
}
fn enc(v: &Value) -> String {
    match v {
        Value::Reference(r) => enc(&r.borrow()),
        Value::Null => "N".into(),
        Value::Bool(b) => format!("B:{}", u8::from(*b)),
        Value::Int(n) => format!("I:{n}"),
        Value::Float(f) => {
            if f.is_nan() {
                "F:NAN".into()
            } else {
                format!("F:{}", hex(&f.to_be_bytes()))
            }
        }
        Value::String(s) => format!("S:{}", hex(s.as_bytes())),
        Value::Array(a) => format!(
            "A({})",
            a.iter()
                .map(|(k, v)| format!(
                    "{}>{};",
                    enc(&match k {
                        ArrayKey::Int(i) => Value::Int(*i),
                        ArrayKey::String(s) => Value::String(s.clone()),
                    }),
                    enc(v)
                ))
                .collect::<String>()
        ),
        _ => panic!("unsupported oracle value"),
    }
}
fn php(v: &Value) -> String {
    match v {
        Value::Reference(r) => php(&r.borrow()),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => {
            if *i == i64::MIN {
                "(-PHP_INT_MAX-1)".into()
            } else {
                i.to_string()
            }
        }
        Value::Float(f) => {
            if f.is_nan() {
                "NAN".into()
            } else if f.is_infinite() {
                if f.is_sign_negative() {
                    "-INF".into()
                } else {
                    "INF".into()
                }
            } else {
                format!("{f:?}")
            }
        }
        Value::String(s) => format!("hex2bin('{}')", hex(s.as_bytes())),
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(|(k, v)| format!(
                    "{}=>{}",
                    php(&match k {
                        ArrayKey::Int(i) => Value::Int(*i),
                        ArrayKey::String(s) => Value::String(s.clone()),
                    }),
                    php(v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("unsupported"),
    }
}
fn s(bytes: &[u8]) -> Value {
    Value::String(PhpString::from(bytes))
}
#[test]
fn php_oracle() {
    let mut cases: Vec<(&str, Vec<Value>)> = Vec::new();
    let mut strings = vec![
        vec![],
        b" Hello World \0\r\n\t\x0c\x0b".to_vec(),
        b"a..z".to_vec(),
        vec![255, 0, 128, 65, 90, 97, 122],
    ];
    for b in 0..=255u8 {
        strings.push(vec![b]);
        strings.push(vec![b, b' ', b, b'A', 0, b]);
    }
    for bytes in &strings {
        for name in [
            "strlen",
            "strtoupper",
            "strtolower",
            "ucfirst",
            "lcfirst",
            "ucwords",
            "strrev",
            "trim",
            "ltrim",
            "rtrim",
            "chop",
            "str_split",
        ] {
            cases.push((name, vec![s(bytes)]));
        }
        for chars in [
            b"".as_slice(),
            b"a..z",
            b"z..a",
            b"..a",
            b"a..",
            b"a..b..c",
            b"\0..\xff",
        ] {
            for name in ["trim", "ltrim", "rtrim", "chop"] {
                cases.push((name, vec![s(bytes), s(chars)]));
            }
        }
        for offset in [i64::MIN, -20, -6, -1, 0, 1, 6, 20, i64::MAX] {
            for len in [
                Value::Null,
                Value::Int(i64::MIN),
                Value::Int(-1),
                Value::Int(0),
                Value::Int(1),
                Value::Int(i64::MAX),
            ] {
                cases.push(("substr", vec![s(bytes), Value::Int(offset), len]));
            }
            for needle in [b"".as_slice(), b"A", b"\0", b"\xff"] {
                for name in ["strpos", "stripos"] {
                    cases.push((name, vec![s(bytes), s(needle), Value::Int(offset)]));
                }
            }
        }
        for n in [-2, -1, 0, 1, 2, 8] {
            cases.push(("str_repeat", vec![s(bytes), Value::Int(n)]));
            cases.push(("str_split", vec![s(bytes), Value::Int(n)]));
            cases.push(("explode", vec![s(b" "), s(bytes), Value::Int(n)]));
        }
        for needle in [b"".as_slice(), b"a", b"A", b"\0", b"\xff"] {
            for name in ["str_contains", "str_starts_with", "str_ends_with"] {
                cases.push((name, vec![s(bytes), s(needle)]));
            }
            for name in ["str_replace", "str_ireplace"] {
                cases.push((name, vec![s(needle), s(b"xyz"), s(bytes)]));
            }
        }
    }
    for name in [
        "ceil",
        "floor",
        "sqrt",
        "exp",
        "log10",
        "log2",
        "log1p",
        "expm1",
        "sin",
        "cos",
        "tan",
        "asin",
        "acos",
        "atan",
        "sinh",
        "cosh",
        "tanh",
        "asinh",
        "acosh",
        "atanh",
        "abs",
        "is_nan",
        "is_finite",
        "is_infinite",
        "deg2rad",
        "rad2deg",
        "log",
    ] {
        for n in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.0,
            0.0,
            -1.0,
            1.0,
            0.5,
            10.0,
            1e20,
        ] {
            cases.push((name, vec![Value::Float(n)]));
        }
    }
    for n in [i64::MIN, -1, 0, 1, i64::MAX] {
        for d in [i64::MIN, -1, 0, 1, i64::MAX] {
            cases.push(("intdiv", vec![Value::Int(n), Value::Int(d)]));
        }
    }
    for name in ["fdiv", "fmod", "hypot", "atan2", "pow"] {
        for n in [-0.0, 0.0, 1.0, -1.0, 2.0, f64::INFINITY, f64::NAN] {
            for d in [-0.0, 0.0, 1.0, -1.0, 2.0, f64::INFINITY, f64::NAN] {
                cases.push((name, vec![Value::Float(n), Value::Float(d)]));
            }
        }
    }
    cases.push(("pi", vec![]));
    for name in ["implode", "join"] {
        let mut a = PhpArray::new();
        for v in [Value::Null, Value::Bool(false), s(b"\xff"), Value::Int(123)] {
            a.push(v).expect("push");
        }
        cases.push((name, vec![s(b"-"), Value::Array(std::rc::Rc::new(a))]));
    }
    for size in 0..25 {
        let mut arr = PhpArray::new();
        for i in 0..size {
            let key = if i % 3 == 0 {
                ArrayKey::String(PhpString::from(format!("k{i}")))
            } else {
                ArrayKey::Int(i - 10)
            };
            arr.insert(
                key,
                match i % 5 {
                    0 => Value::Null,
                    1 => Value::Int(i),
                    2 => Value::Bool(false),
                    3 => Value::Float(i as f64),
                    _ => Value::from("x"),
                },
            );
        }
        let arr = Value::Array(std::rc::Rc::new(arr));
        for name in [
            "array_keys",
            "array_values",
            "array_key_first",
            "array_key_last",
            "array_first",
            "array_last",
            "array_is_list",
        ] {
            cases.push((name, vec![arr.clone()]));
        }
        for preserve in [false, true] {
            cases.push(("array_reverse", vec![arr.clone(), Value::Bool(preserve)]));
            for offset in [i64::MIN, -30, -1, 0, 1, 20, i64::MAX] {
                for length in [
                    Value::Null,
                    Value::Int(i64::MIN),
                    Value::Int(-1),
                    Value::Int(0),
                    Value::Int(10),
                    Value::Int(i64::MAX),
                ] {
                    cases.push((
                        "array_slice",
                        vec![
                            arr.clone(),
                            Value::Int(offset),
                            length,
                            Value::Bool(preserve),
                        ],
                    ));
                }
            }
        }
        for needle in [
            Value::Null,
            Value::Bool(false),
            Value::Int(1),
            Value::Float(1.0),
            Value::from("x"),
            Value::from("k0"),
        ] {
            for strict in [false, true] {
                for name in ["in_array", "array_search"] {
                    cases.push((name, vec![needle.clone(), arr.clone(), Value::Bool(strict)]));
                }
                cases.push((
                    "array_keys",
                    vec![arr.clone(), needle.clone(), Value::Bool(strict)],
                ));
            }
            for name in ["array_key_exists", "key_exists"] {
                cases.push((name, vec![needle.clone(), arr.clone()]));
            }
        }
    }
    // php-src ext/standard/tests/strings/trim_basic.phpt.
    for (text, mask) in [
        (b"!===Hello World===!".as_slice(), b"=!".as_slice()),
        (b"!===Hello World===!", b"Hdle"),
        (b"\x0a\x0dExample string\x0a\x0d", b"\x00..\x1f"),
    ] {
        cases.push(("trim", vec![s(text), s(mask)]));
    }
    for name in ["implode", "join"] {
        cases.push((name, vec![s(b"x")]));
        let empty = Value::Array(std::rc::Rc::new(PhpArray::new()));
        cases.push((name, vec![empty.clone(), empty.clone()]));
        cases.push((name, vec![empty, Value::Null]));
    }
    for name in ["min", "max"] {
        let vals = vec![
            Value::Null,
            Value::Bool(false),
            Value::Int(-1),
            Value::Int(i64::MAX),
            Value::Float(f64::NAN),
            Value::from("x"),
            Value::from("12"),
        ];
        for a in &vals {
            cases.push((name, vec![a.clone()]));
            for b in &vals {
                cases.push((name, vec![a.clone(), b.clone()]));
            }
        }
        cases.push((name, vec![Value::Array(std::rc::Rc::new(PhpArray::new()))]));
    }
    for name in ["str_replace", "str_ireplace"] {
        for search_array in [false, true] {
            for replace_array in [false, true] {
                for subject_array in [false, true] {
                    let array = |v: Value| {
                        let mut a = PhpArray::new();
                        a.insert(ArrayKey::String(PhpString::from("key")), v);
                        a.push(Value::from("A")).expect("push");
                        Value::Array(std::rc::Rc::new(a))
                    };
                    let search = if search_array {
                        array(Value::from("a"))
                    } else {
                        Value::from("a")
                    };
                    let replacement = if replace_array {
                        array(Value::from("X"))
                    } else {
                        Value::from("X")
                    };
                    let subject = if subject_array {
                        array(Value::from("aAa"))
                    } else {
                        Value::from("aAa")
                    };
                    cases.push((
                        name,
                        vec![
                            search,
                            replacement,
                            subject,
                            Value::Reference(std::rc::Rc::new(std::cell::RefCell::new(
                                Value::Int(99),
                            ))),
                        ],
                    ));
                }
            }
        }
    }
    for name in [
        "strlen",
        "strtoupper",
        "strtolower",
        "ucfirst",
        "lcfirst",
        "strrev",
        "trim",
        "ltrim",
        "rtrim",
        "chop",
    ] {
        for v in [
            Value::Null,
            Value::Bool(false),
            Value::Bool(true),
            Value::Int(i64::MIN),
            Value::Int(i64::MAX),
            Value::Float(-0.0),
            Value::Float(f64::NAN),
            Value::Float(f64::INFINITY),
            Value::Array(std::rc::Rc::new(PhpArray::new())),
        ] {
            cases.push((name, vec![v]));
        }
        cases.push((name, vec![]));
        cases.push((
            name,
            vec![Value::from("x"), Value::from("y"), Value::from("z")],
        ));
    }
    run(cases);
}
fn run(cases: Vec<(&str, Vec<Value>)>) {
    let php_bin = "/opt/homebrew/bin/php";
    if !std::path::Path::new(php_bin).exists() {
        eprintln!("PHP missing; skipping oracle");
        return;
    }
    let mut script = String::from(
        r#"<?php
function enc($v){if(is_null($v))return 'N';if(is_bool($v))return 'B:'.(int)$v;if(is_int($v))return 'I:'.$v;if(is_float($v))return is_nan($v)?'F:NAN':'F:'.bin2hex(pack('E',$v));if(is_string($v))return 'S:'.bin2hex($v);$s='A(';foreach($v as $k=>$x)$s.=enc($k).'>'.enc($x).';';return $s.')';}
set_error_handler(function($n,$m)use(&$diag){$diag.='|'.$n.':'.bin2hex($m);return true;});
function invoke($name,$args){global $diag;$diag='';$refs=[];if(in_array($name,['str_replace','str_ireplace'])&&count($args)>3){$refs[3]=&$args[3];}ob_start();try{$v=enc($name(...$args));}catch(Throwable $e){$v='E:'.get_class($e).':'.bin2hex($e->getMessage());}$out=ob_get_clean();foreach($refs as $x)$v.='|R:'.enc($x);echo $v,$diag,'|O:',bin2hex($out),"\n";}
"#,
    );
    for (name, a) in &cases {
        writeln!(
            script,
            "invoke('{name}',[{}]);",
            a.iter().map(php).collect::<Vec<_>>().join(",")
        )
        .expect("format");
    }
    let dir = std::path::Path::new(
        "/private/tmp/claude-501/-Users-gijsr--t3-worktrees-phera-t3-f1d03fb9/8823bb04-57b9-49e9-a9e2-fe9f0dede31c/scratchpad/stdlib",
    );
    std::fs::create_dir_all(dir).expect("mkdir");
    let path = dir.join("standard-oracle.php");
    std::fs::write(&path, script).expect("write");
    let output = Command::new(php_bin)
        .arg("-n")
        .arg("-d")
        .arg("memory_limit=-1")
        .arg(path)
        .output()
        .expect("php");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).expect("ascii");
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines.len(), cases.len());
    let mut groups = std::collections::BTreeMap::new();
    for (name, _) in &cases {
        let group = if name.starts_with("array_") || ["in_array", "key_exists"].contains(name) {
            "arrays"
        } else if [
            "ceil",
            "floor",
            "sqrt",
            "exp",
            "log10",
            "log2",
            "log1p",
            "expm1",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "sinh",
            "cosh",
            "tanh",
            "asinh",
            "acosh",
            "atanh",
            "abs",
            "is_nan",
            "is_finite",
            "is_infinite",
            "deg2rad",
            "rad2deg",
            "log",
            "intdiv",
            "fdiv",
            "fmod",
            "hypot",
            "atan2",
            "pow",
            "pi",
            "min",
            "max",
        ]
        .contains(name)
        {
            "math"
        } else {
            "strings"
        };
        *groups.entry(group).or_insert(0usize) += 1;
    }
    eprintln!("Oracle group counts: {groups:?}");
    let mut registry = Registry::new();
    registry.load(&Standard);
    let mut failures = Vec::new();
    for ((name, a), expected) in cases.iter().zip(lines) {
        let mut c = TestContext::default();
        let result = call(&registry, &mut c, name, a.clone());
        let mut actual = match result {
            Ok(v) => enc(&v),
            Err(NativeError::Throw { class, message }) => {
                format!("E:{class}:{}", hex(message.as_bytes()))
            }
            Err(e) => panic!("{e:?}"),
        };
        for v in a {
            if let Value::Reference(r) = v {
                write!(actual, "|R:{}", enc(&r.borrow())).expect("format");
            }
        }
        for (kind, message) in c.diagnostics.entries {
            let code = match kind {
                value::DiagnosticKind::Warning => 2,
                value::DiagnosticKind::Notice => 8,
                value::DiagnosticKind::Deprecated => 8192,
            };
            write!(actual, "|{code}:{}", hex(message.as_bytes())).expect("format");
        }
        write!(actual, "|O:{}", hex(&c.output)).expect("format");
        if actual != expected {
            failures.push(format!(
                "{name}({a:?})\n actual {actual}\n expect {expected}"
            ));
        }
    }
    eprintln!(
        "{} oracle cases, {} mismatches",
        cases.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
