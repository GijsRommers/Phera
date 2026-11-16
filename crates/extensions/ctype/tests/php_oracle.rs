//! Generated PHP executable oracle cases, including diagnostics and output.

use phera_ext_ctype::Ctype;
use phera_extension_api::{
    ArrayKey, NativeError, PhpArray, PhpString, Registry, Value,
    testing::{TestContext, call},
    value,
};
use std::{fmt::Write, process::Command};
fn hex(s: &[u8]) -> String {
    s.iter().map(|b| format!("{b:02x}")).collect()
}
fn enc(v: &Value) -> String {
    match v {
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
    let mut cases = Vec::new();
    for name in [
        "ctype_alnum",
        "ctype_alpha",
        "ctype_cntrl",
        "ctype_digit",
        "ctype_graph",
        "ctype_lower",
        "ctype_print",
        "ctype_punct",
        "ctype_space",
        "ctype_upper",
        "ctype_xdigit",
    ] {
        for b in 0..=255u8 {
            cases.push((name, vec![s(&[b])]));
            for c in [0, 9, 10, 11, 12, 32, 48, 65, 97, 127, 128, 255] {
                cases.push((name, vec![s(&[b, c])]));
            }
        }
        for i in -300..=512 {
            cases.push((name, vec![Value::Int(i)]));
        }
        for v in [
            Value::Null,
            Value::Bool(false),
            Value::Bool(true),
            Value::Float(1.0),
            Value::Float(f64::NAN),
            Value::Float(f64::INFINITY),
            Value::Int(i64::MIN),
            Value::Int(i64::MAX),
            s(b""),
            Value::Array(std::rc::Rc::new(PhpArray::new())),
        ] {
            cases.push((name, vec![v]));
        }
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
setlocale(LC_CTYPE, 'C');
function enc($v){if(is_null($v))return 'N';if(is_bool($v))return 'B:'.(int)$v;if(is_int($v))return 'I:'.$v;if(is_float($v))return is_nan($v)?'F:NAN':'F:'.bin2hex(pack('E',$v));if(is_string($v))return 'S:'.bin2hex($v);$s='A(';foreach($v as $k=>$x)$s.=enc($k).'>'.enc($x).';';return $s.')';}
set_error_handler(function($n,$m)use(&$diag){$diag.='|'.$n.':'.bin2hex($m);return true;});
function invoke($name,$args){global $diag;$diag='';ob_start();try{$v=enc($name(...$args));}catch(Throwable $e){$v='E:'.get_class($e).':'.bin2hex($e->getMessage());}$out=ob_get_clean();echo $v,$diag,'|O:',bin2hex($out),"\n";}
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
    let path = dir.join("ctype-oracle.php");
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
    let mut registry = Registry::new();
    registry.load(&Ctype);
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
