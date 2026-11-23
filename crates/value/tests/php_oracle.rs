//! Oracle tests (groups B + C): every operator, conversion and comparison
//! is evaluated over a value matrix in real PHP 8.5 and compared line by
//! line against this crate. One line per case in an unambiguous encoding
//! (`<tag> = <result> | <diag>...`, see `enc_value`); failures show the
//! first 30 mismatches.

use std::process::Command;
use std::rc::Rc;

use phera_value::*;

// ---------------------------------------------------------------------------
// encoding: must match enc()/encf()/enck() in the generated PHP byte for byte
// ---------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn enc_float(f: f64) -> String {
    if f.is_nan() {
        return "F:NAN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "F:INF".to_string()
        } else {
            "F:-INF".to_string()
        };
    }
    if f == 0.0 {
        return if f.is_sign_negative() {
            "F:-0".to_string()
        } else {
            "F:0".to_string()
        };
    }
    format!("F:{}", float_repr(f))
}

fn enc_key(k: &ArrayKey) -> String {
    match k {
        ArrayKey::Int(i) => format!("I:{i}"),
        ArrayKey::Interned(s) => format!("S:{}", hex(s.as_bytes())),
        ArrayKey::String(s) => format!("S:{}", hex(s.as_bytes())),
    }
}

fn enc_value(v: &Value) -> String {
    match v {
        Value::Null => "N".to_string(),
        Value::Bool(b) => format!("B:{}", u8::from(*b)),
        Value::Int(i) => format!("I:{i}"),
        Value::Float(f) => enc_float(*f),
        Value::InternedStr(s) => format!("S:{}", hex(s.as_bytes())),
        Value::String(s) => format!("S:{}", hex(s.as_bytes())),
        Value::Array(a) => {
            let mut s = String::from("A(");
            for (k, val) in a.iter() {
                s.push_str(&enc_key(&k));
                s.push('>');
                s.push_str(&enc_value(val));
                s.push(';');
            }
            s.push(')');
            s
        }
        Value::Object(o) => {
            let props = o.to_array();
            let mut s = format!("O:{}(", o.class.name());
            for (k, val) in props.iter() {
                s.push_str(&enc_key(&k));
                s.push('>');
                s.push_str(&enc_value(val));
                s.push(';');
            }
            s.push(')');
            s
        }
        Value::Resource(_) => unreachable!("no resources in the matrix"),
        Value::Reference(_) => unreachable!("no references in the matrix"),
    }
}

fn enc_err(e: &OpError) -> String {
    let class = e.kind.class_name();
    format!("E:{class}:{}", e.message)
}

fn enc_diag(entries: &[(DiagnosticKind, String)]) -> String {
    let mut s = String::new();
    for (kind, msg) in entries {
        let tag = match kind {
            DiagnosticKind::Warning => "W:",
            DiagnosticKind::Notice => "N:",
            DiagnosticKind::Deprecated => "D:",
        };
        s.push('|');
        s.push_str(tag);
        s.push_str(msg);
    }
    s
}

// ---------------------------------------------------------------------------
// running PHP
// ---------------------------------------------------------------------------

fn run_php(what: &str, script: &str) -> Option<Vec<String>> {
    let php = phera_extension_api::testing::oracle::find_php()?;
    let dir = std::env::temp_dir().join("phera-value-oracle");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("{what}.php"));
    std::fs::write(&file, script).expect("write oracle script");
    let out = Command::new(&php)
        .arg("-n")
        .arg("-d")
        .arg("error_reporting=-1")
        .arg(&file)
        .output()
        .expect("run php");
    if !out.status.success() {
        panic!(
            "php exited with {:?}\nstderr:\n{}\nstdout head:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
                .chars()
                .take(2000)
                .collect::<String>()
        );
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.to_string())
            .collect(),
    )
}

fn compare_lines(expected_php: &[String], produced_rust: Vec<String>, what: &str) {
    assert_eq!(
        expected_php.len(),
        produced_rust.len(),
        "{what}: line count differs (php {}, rust {})",
        expected_php.len(),
        produced_rust.len()
    );
    let mut mismatches = Vec::new();
    for (i, (p, r)) in expected_php.iter().zip(produced_rust.iter()).enumerate() {
        if p != r {
            mismatches.push(format!("#{i}\n  php : {p}\n  rust: {r}"));
        }
    }
    if !mismatches.is_empty() {
        let shown = mismatches
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "{what}: {} MISMATCHES out of {}\n{}",
            mismatches.len(),
            expected_php.len(),
            shown
        );
    }
    println!("{what}: {} cases, zero mismatches", expected_php.len());
}

// ---------------------------------------------------------------------------
// the value matrix
// ---------------------------------------------------------------------------

struct VSpec {
    php: String,
    value: Value,
}

struct DummyClass {
    name: String,
}
impl ClassHandle for DummyClass {
    fn name(&self) -> &str {
        &self.name
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn dummy_object(name: &str, props: &[(&str, Value)], id: u32) -> Value {
    let class = Rc::new(DummyClass {
        name: name.to_string(),
    }) as Rc<dyn ClassHandle>;
    let o = Object::new(id, class);
    for (k, v) in props {
        o.set(ArrayKey::str(*k), v.clone());
    }
    Value::Object(Rc::new(o))
}

/// Builds a list array with sequential integer keys, like `[1, 2]`.
fn push_list(items: Vec<Value>) -> Value {
    let mut a = PhpArray::new();
    for v in items {
        a.push(v).unwrap();
    }
    Value::Array(Rc::new(a))
}

/// Builds an array with normalised string keys, like `['a' => 1]`.
fn smap(entries: &[(&str, Value)]) -> Value {
    let mut a = PhpArray::new();
    for (k, v) in entries {
        a.insert(ArrayKey::from_value(&Value::from(*k)).unwrap(), v.clone());
    }
    Value::Array(Rc::new(a))
}

fn values() -> Vec<VSpec> {
    let mut vals: Vec<VSpec> = Vec::new();
    let mut add = |php: &str, value: Value| {
        vals.push(VSpec {
            php: php.to_string(),
            value,
        })
    };

    // scalars: null / bool / int
    add("null", Value::Null);
    add("true", Value::Bool(true));
    add("false", Value::Bool(false));
    add("0", Value::Int(0));
    add("1", Value::Int(1));
    add("-1", Value::Int(-1));
    add("PHP_INT_MAX", Value::Int(i64::MAX));
    add("PHP_INT_MIN", Value::Int(i64::MIN));

    // floats
    add("0.0", Value::Float(0.0));
    add("-0.0", Value::Float(-0.0));
    add("1.5", Value::Float(1.5));
    add("-1.5", Value::Float(-1.5));
    add("NAN", Value::Float(f64::NAN));
    add("INF", Value::Float(f64::INFINITY));
    add("-INF", Value::Float(f64::NEG_INFINITY));
    add("0.1", Value::Float(0.1));
    add("0.1 + 0.2", Value::Float(0.1 + 0.2));
    add("1e100", Value::Float(1e100));
    add("-1e100", Value::Float(-1e100));
    add("1e14", Value::Float(1e14));
    add("1e15", Value::Float(1e15));
    add("1e-5", Value::Float(1e-5));
    add("1e-7", Value::Float(1e-7));
    add("9.223372036854775E18", Value::Float(9.223372036854775e18));
    add("9.5e18", Value::Float(9.5e18));
    add("1e300", Value::Float(1e300));
    add("5e-324", Value::Float(5e-324));
    add("2.5", Value::Float(2.5));

    // strings (PHP expressions built from Rust's Debug escaping; these are
    // all ASCII, or explicitly escaped below)
    for s in [
        "",
        "0",
        "1",
        "-1",
        "01",
        "1.0",
        "1e3",
        "1e9",
        " 1",
        "1 ",
        " 1 ",
        "abc",
        "1abc",
        "abc1",
        "0x1A",
        ".5",
        "5.",
        "-.5",
        "-.5e2",
        "+5",
        "-0",
        "+0",
        "08",
        "-",
        "+",
        ".",
        "e",
        "e5",
        "nan",
        "inf",
        "1_0",
        "1,5",
        "a",
        "A",
        "Z",
        "z",
        "zz",
        "Az",
        "Az9",
        "9z",
        "z-",
        "\t\n 42 \n",
        "9223372036854775807",
        "-9223372036854775808",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        add(&format!("{s:?}"), Value::String(PhpString::from(s)));
    }
    add(
        "\"\\x80\\xff\"",
        Value::String(PhpString::from(vec![0x80u8, 0xff])),
    );

    // arrays
    add("[]", push_list(vec![]));
    add("[1,2]", push_list(vec![Value::Int(1), Value::Int(2)]));
    add("[2,1]", push_list(vec![Value::Int(2), Value::Int(1)]));
    add("['a'=>1]", smap(&[("a", Value::Int(1))]));
    add(
        "['a'=>1,'b'=>2]",
        smap(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
    );
    add(
        "['b'=>2,'a'=>1]",
        smap(&[("b", Value::Int(2)), ("a", Value::Int(1))]),
    );
    add(
        "[1,[2,3]]",
        push_list(vec![
            Value::Int(1),
            push_list(vec![Value::Int(2), Value::Int(3)]),
        ]),
    );
    add(
        "['x'=>['y'=>1]]",
        smap(&[("x", smap(&[("y", Value::Int(1))]))]),
    );
    add("[-1=>'n',3=>'m']", {
        let mut a = PhpArray::new();
        a.insert(ArrayKey::Int(-1), Value::String(PhpString::from("n")));
        a.insert(ArrayKey::Int(3), Value::String(PhpString::from("m")));
        Value::Array(Rc::new(a))
    });
    add(
        "[1,1.5,null,'s',true]",
        push_list(vec![
            Value::Int(1),
            Value::Float(1.5),
            Value::Null,
            Value::String(PhpString::from("s")),
            Value::Bool(true),
        ]),
    );

    // objects: the same C{p:1,q:2} instance twice (=== case), a twin,
    // a D{p:2}, and a stdClass{x:'y'}. PHP builds these in the preamble.
    let oa = dummy_object("C", &[("p", Value::Int(1)), ("q", Value::Int(2))], 1);
    add("$oa", oa.clone());
    add("$oa2", oa);
    add(
        "$ob",
        dummy_object("C", &[("p", Value::Int(1)), ("q", Value::Int(2))], 2),
    );
    add("$od", dummy_object("D", &[("p", Value::Int(2))], 3));
    add(
        "$os",
        dummy_object("stdClass", &[("x", Value::String(PhpString::from("y")))], 4),
    );

    vals
}

// ---------------------------------------------------------------------------
// operators
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Seq,
    Cmp,
    Lt,
    Le,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Cat,
}

impl Bin {
    const ALL: [Bin; 17] = [
        Bin::Add,
        Bin::Sub,
        Bin::Mul,
        Bin::Div,
        Bin::Mod,
        Bin::Pow,
        Bin::Eq,
        Bin::Seq,
        Bin::Cmp,
        Bin::Lt,
        Bin::Le,
        Bin::Shl,
        Bin::Shr,
        Bin::And,
        Bin::Or,
        Bin::Xor,
        Bin::Cat,
    ];
    fn sigil(self) -> &'static str {
        match self {
            Bin::Add => "+",
            Bin::Sub => "-",
            Bin::Mul => "*",
            Bin::Div => "/",
            Bin::Mod => "%",
            Bin::Pow => "**",
            Bin::Eq => "==",
            Bin::Seq => "===",
            Bin::Cmp => "<=>",
            Bin::Lt => "<",
            Bin::Le => "<=",
            Bin::Shl => "<<",
            Bin::Shr => ">>",
            Bin::And => "&",
            Bin::Or => "|",
            Bin::Xor => "^",
            Bin::Cat => ".",
        }
    }
    fn apply(self, a: &Value, b: &Value, d: &mut CollectDiagnostics) -> Result<Value, OpError> {
        match self {
            Bin::Add => add(a, b, d),
            Bin::Sub => sub(a, b, d),
            Bin::Mul => mul(a, b, d),
            Bin::Div => div(a, b, d),
            Bin::Mod => modulo(a, b, d),
            Bin::Pow => pow(a, b, d),
            Bin::Eq => loose_equals(a, b, d),
            Bin::Seq => strict_equals(a, b, d),
            Bin::Cmp => compare_op(a, b, d),
            Bin::Lt => less(a, b, d),
            Bin::Le => less_equal(a, b, d),
            Bin::Shl => shift_left(a, b, d),
            Bin::Shr => shift_right(a, b, d),
            Bin::And => bitwise_and(a, b, d),
            Bin::Or => bitwise_or(a, b, d),
            Bin::Xor => bitwise_xor(a, b, d),
            Bin::Cat => concat(a, b, d),
        }
    }
}

#[derive(Clone, Copy)]
enum Un {
    Neg,
    Plus,
    Not,
    Inc,
    Dec,
}

impl Un {
    const ALL: [Un; 5] = [Un::Neg, Un::Plus, Un::Not, Un::Inc, Un::Dec];
    fn tag(self) -> &'static str {
        match self {
            Un::Neg => "neg",
            Un::Plus => "plus",
            Un::Not => "not",
            Un::Inc => "inc",
            Un::Dec => "dec",
        }
    }
    fn php_call(self) -> &'static str {
        match self {
            Un::Neg => "negv($V[$i])",
            Un::Plus => "posv($V[$i])",
            Un::Not => "notv($V[$i])",
            Un::Inc => "incv($V[$i])",
            Un::Dec => "decv($V[$i])",
        }
    }
    fn apply(self, a: &Value, d: &mut CollectDiagnostics) -> Result<Value, OpError> {
        match self {
            Un::Neg => negate(a, d),
            Un::Plus => unary_plus(a, d),
            Un::Not => bitwise_not(a, d),
            Un::Inc => increment(a, d),
            Un::Dec => decrement(a, d),
        }
    }
}

#[derive(Clone, Copy)]
enum Conv {
    Bool,
    Int,
    Float,
    StringC,
    Array,
    Gettype,
    Dbgtype,
}

impl Conv {
    const ALL: [Conv; 7] = [
        Conv::Bool,
        Conv::Int,
        Conv::Float,
        Conv::StringC,
        Conv::Array,
        Conv::Gettype,
        Conv::Dbgtype,
    ];
    fn tag(self) -> &'static str {
        match self {
            Conv::Bool => "bool",
            Conv::Int => "int",
            Conv::Float => "float",
            Conv::StringC => "string",
            Conv::Array => "array",
            Conv::Gettype => "gettype",
            Conv::Dbgtype => "dbgtype",
        }
    }
    fn php_call(self) -> &'static str {
        match self {
            Conv::Bool => "convb($V[$i])",
            Conv::Int => "convi($V[$i])",
            Conv::Float => "convf($V[$i])",
            Conv::StringC => "convs($V[$i])",
            Conv::Array => "conva($V[$i])",
            Conv::Gettype => "gettype($V[$i])",
            Conv::Dbgtype => "get_debug_type($V[$i])",
        }
    }
    fn apply(self, a: &Value, d: &mut CollectDiagnostics) -> Result<Value, OpError> {
        Ok(match self {
            Conv::Bool => Value::Bool(cast_to_bool(a, d)),
            Conv::Int => Value::Int(to_int(a, d)?),
            Conv::Float => Value::Float(to_float(a, d)?),
            Conv::StringC => Value::String(to_php_string(a, d)?),
            Conv::Array => Value::Array(Rc::new(to_array(a, d))),
            Conv::Gettype => Value::String(PhpString::from(a.gettype_name())),
            Conv::Dbgtype => Value::String(PhpString::from(a.type_name())),
        })
    }
}

/// Cast helpers: PHP's own `(int)$object` warns and returns 1, which this
/// crate reports as an error per its contract, so those rows are skipped;
/// thin wrappers keep the rest of the matrix uniform.
fn php_preamble() -> String {
    r#"<?php
$ERRBUF = [];
set_error_handler(function ($no, $msg) {
    $GLOBALS['ERRBUF'][] = errtag($no) . $msg;
    return true;
});
function errtag($no) {
    return match ($no) {
        2, 512 => 'W:',
        8, 1024 => 'N:',
        8192, 16384 => 'D:',
        default => "X$no:",
    };
}
function enck($k) { return is_int($k) ? "I:$k" : 'S:' . bin2hex($k); }
function encf($f) {
    if (is_nan($f)) return 'F:NAN';
    if (is_infinite($f)) return $f > 0 ? 'F:INF' : 'F:-INF';
    if ($f == 0.0) return (string) $f === '-0' ? 'F:-0' : 'F:0';
    return 'F:' . sprintf('%.*H', -1, $f);
}
function enc($v) {
    if (is_object($v)) {
        $s = 'O:' . get_class($v) . '(';
        foreach ($v as $k => $pv) { $s .= enck($k) . '>' . enc($pv) . ';'; }
        return $s . ')';
    }
    if (is_array($v)) {
        $s = 'A(';
        foreach ($v as $k => $pv) { $s .= enck($k) . '>' . enc($pv) . ';'; }
        return $s . ')';
    }
    if (is_int($v)) return "I:$v";
    if (is_float($v)) return encf($v);
    if (is_string($v)) return 'S:' . bin2hex($v);
    if (is_bool($v)) return $v ? 'B:1' : 'B:0';
    if (is_null($v)) return 'N';
    return 'OTHER';
}
function runop($tag, $f) {
    global $ERRBUF;
    $ERRBUF = [];
    try {
        $s = enc($f());
    } catch (Throwable $e) {
        $s = 'E:' . get_class($e) . ':' . $e->getMessage();
    }
    $line = $tag . '=' . $s;
    foreach ($ERRBUF as $w) { $line .= '|' . $w; }
    echo $line, "\n";
}
function negv($v) { return -$v; }
function posv($v) { return +$v; }
function notv($v) { return ~$v; }
function incv($v) { $v++; return $v; }
function decv($v) { $v--; return $v; }
function convb($v) { return (bool) $v; }
function convi($v) { return (int) $v; }
function convf($v) { return (float) $v; }
function convs($v) { return (string) $v; }
function conva($v) { return (array) $v; }
function keyof($v) { $z = []; $z[$v] = 1; return $z; }
class C { public $p = 1; public $q; }
class D { public $p = 2; }
$oa = new C; $oa->q = 2;
$oa2 = $oa;
$ob = new C; $ob->q = 2;
$od = new D;
$os = new stdClass; $os->x = 'y';
$V = [];
"#
    .to_string()
}

fn run_case(run: impl FnOnce(&mut CollectDiagnostics) -> Result<Value, OpError>) -> String {
    let mut d = CollectDiagnostics::default();
    let out = match run(&mut d) {
        Ok(v) => enc_value(&v),
        Err(e) => enc_err(&e),
    };
    format!("{out}{}", enc_diag(&d.entries))
}

fn emit_cases(vals: &[VSpec]) -> Vec<Case> {
    // Case enumeration shared by generator and evaluator below.
    let n = vals.len();
    let mut cases = Vec::new();
    for op in Bin::ALL {
        for i in 0..n {
            for j in 0..n {
                cases.push(Case::Bin(op, i, j));
            }
        }
    }
    for op in Un::ALL {
        for i in 0..n {
            cases.push(Case::Un(op, i));
        }
    }
    for op in Conv::ALL {
        for i in 0..n {
            cases.push(Case::Conv(op, i));
        }
    }
    for (i, v) in vals.iter().enumerate() {
        if v.value.is_string() {
            cases.push(Case::IsNum(i));
        }
    }
    cases
}

#[derive(Clone, Copy)]
enum Case {
    Bin(Bin, usize, usize),
    Un(Un, usize),
    Conv(Conv, usize),
    IsNum(usize),
}

#[test]
fn oracle_operators_and_conversions() {
    let vals = values();
    let cases = emit_cases(&vals);
    let n = vals.len();

    let mut php = php_preamble();
    for (i, spec) in vals.iter().enumerate() {
        php.push_str(&format!("$V[{i}] = {};\n", spec.php));
    }
    // Binary ops go through one operator table with foreach loops: a
    // one-line closure per case would make PHP compile ~100k op arrays and
    // blow its memory limit.
    let mut table = String::from("$BT = [\n");
    for op in Bin::ALL {
        table.push_str(&format!(
            "    {:?} => fn($a, $b) => $a {} $b,\n",
            op.sigil(),
            op.sigil()
        ));
    }
    table.push_str("];\n");
    php.push_str(&table);
    php.push_str(
        "foreach ($BT as $sig => $fn) {
    foreach ($V as $i => $a) {
        foreach ($V as $j => $b) {
            runop($sig . '|' . $i . '|' . $j, fn() => $fn($a, $b));
        }
    }
}\n",
    );
    for op in Un::ALL {
        for i in 0..n {
            php.push_str(&format!(
                "$i = {i}; runop('{}|{i}', fn() => {});\n",
                op.tag(),
                op.php_call()
            ));
        }
    }
    for op in Conv::ALL {
        for i in 0..n {
            php.push_str(&format!(
                "$i = {i}; runop('{}|{i}', fn() => {});\n",
                op.tag(),
                op.php_call()
            ));
        }
    }
    for (i, v) in vals.iter().enumerate() {
        if v.value.is_string() {
            php.push_str(&format!(
                "$i = {i}; runop('isnum|{i}', fn() => is_numeric($V[$i]));\n"
            ));
        }
    }

    let what = "operators+conversions";
    let lines = match run_php(what, &php) {
        Some(l) => l,
        None => {
            eprintln!("php binary not found; skipping operator oracle test");
            return;
        }
    };

    let produced = cases
        .iter()
        .map(|case| {
            let tag = match *case {
                Case::Bin(op, i, j) => format!("{}|{i}|{j}", op.sigil()),
                Case::Un(op, i) => format!("{}|{i}", op.tag()),
                Case::Conv(op, i) => format!("{}|{i}", op.tag()),
                Case::IsNum(i) => format!("isnum|{i}"),
            };
            let body = match *case {
                Case::Bin(op, i, j) => run_case(|d| op.apply(&vals[i].value, &vals[j].value, d)),
                Case::Un(op, i) => run_case(|d| op.apply(&vals[i].value, d)),
                Case::Conv(op, i) => run_case(|d| op.apply(&vals[i].value, d)),
                Case::IsNum(i) => run_case(|_d| {
                    if let Value::String(s) = &vals[i].value {
                        Ok(Value::Bool(is_numeric(s.as_bytes())))
                    } else {
                        unreachable!()
                    }
                }),
            };
            format!("{tag}={body}")
        })
        .collect();

    compare_lines(&lines, produced, "operators+conversions");
}

// ---------------------------------------------------------------------------
// group C: array keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_array_keys() {
    let vals = values();
    let n = vals.len();

    let mut php = php_preamble();
    for (i, spec) in vals.iter().enumerate() {
        php.push_str(&format!("$V[{i}] = {};\n", spec.php));
    }
    for i in 0..n {
        php.push_str(&format!(
            "$i = {i}; runop('key|{i}', fn() => keyof($V[$i]));\n"
        ));
    }

    let what = "array keys";
    let lines = match run_php(what, &php) {
        Some(l) => l,
        None => {
            eprintln!("php binary not found; skipping array-key oracle test");
            return;
        }
    };

    let produced = (0..n)
        .map(|i| {
            let body = run_case(|d| {
                let key = ArrayKey::from_value_diag(&vals[i].value, d)?;
                let mut a = PhpArray::new();
                a.insert(key, Value::Int(1));
                Ok(Value::Array(Rc::new(a)))
            });
            format!("key|{i}={body}")
        })
        .collect();

    compare_lines(&lines, produced, "array keys");
}

// ---------------------------------------------------------------------------
// float formatting sweep: deterministic pseudo-random doubles through both
// converters, compared against PHP's zend_gcvt via sprintf("%.*H")
// ---------------------------------------------------------------------------

/// xorshift64*: identical sequence on both sides.
fn lcg64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn oracle_float_formats() {
    let mut st: u64 = 0x2545F4914F6CDD1D;
    let mut floats: Vec<f64> = Vec::new();
    let mut php_entries: Vec<String> = Vec::new();
    for _ in 0..600 {
        let bits: u64 = {
            let a = lcg64(&mut st) & 1;
            let b = lcg64(&mut st) >> 52;
            let c = lcg64(&mut st) >> 12;
            a << 63 | b << 52 | c & ((1u64 << 52) - 1)
        };
        let f = f64::from_bits(bits);
        floats.push(f);
        // hex bit pattern so both sides see the identical double
        php_entries.push(format!("unpack('E', hex2bin('{:016x}'))[1]", bits));
    }

    let mut script = String::from("<?php\n$F = [\n");
    for e in &php_entries {
        script.push_str(e);
        script.push_str(",\n");
    }
    script.push_str("];\n");
    script.push_str(
        "foreach ($F as $i => $f) {\n    echo 'p14|', $i, '=S:', bin2hex(sprintf('%.*H', 14, $f)), \"\\n\";\n}\n",
    );
    script.push_str(
        "foreach ($F as $i => $f) {\n    echo 'repr|', $i, '=S:', bin2hex(sprintf('%.*H', -1, $f)), \"\\n\";\n}\n",
    );

    let lines = match run_php("float formats", &script) {
        Some(l) => l,
        None => {
            eprintln!("php binary not found; skipping float-format oracle test");
            return;
        }
    };

    let mut produced = Vec::new();
    for (i, f) in floats.iter().enumerate() {
        produced.push(format!(
            "p14|{i}=S:{}",
            hex(float_to_string_precision14(*f).as_bytes())
        ));
    }
    for (i, f) in floats.iter().enumerate() {
        let text = if f.is_nan() {
            // %H spells it "NaN"; float_repr's contract says "NAN".
            "NaN".to_string()
        } else if f.is_infinite() {
            "INF".to_string()
        } else {
            float_repr(*f)
        };
        produced.push(format!("repr|{i}=S:{}", hex(text.as_bytes())));
    }
    compare_lines(&lines, produced, "float formats");
}
