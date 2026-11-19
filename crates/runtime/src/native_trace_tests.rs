//! Native entry failures are compared with PHP, including `PDO->query($sql)`.

use std::fmt::Write;

use phera_extension_api::testing::oracle;
use phera_extension_api::{ClassEntry, Registry, Visibility};

use super::Runtime;

const PREAMBLE: &str = r#"<?php
ini_set('zend.exception_ignore_args', '0');
function frameProbe(string $label, Closure $call): void {
    try {
        $call();
        echo $label, "\treturned\n";
    } catch (Throwable $error) {
        $frame = $error->getTrace()[0] ?? [];
        echo $label, "\tthrew\t", json_encode([
            $frame['function'] ?? null,
            $frame['class'] ?? null,
            $frame['type'] ?? null,
            count($frame['args'] ?? []),
            explode("\n", $error->getTraceAsString(), 2)[0],
        ], JSON_INVALID_UTF8_SUBSTITUTE), "\n";
    }
}
"#;

const BODY_ERRORS: &[(&str, &str)] = &[
    ("format sprintf", "sprintf('%s')"),
    ("format printf", "printf('%s %s', 'x')"),
    ("format vsprintf", "vsprintf('%s %s', ['x'])"),
    ("format vprintf", "vprintf('%s %s', ['x'])"),
    ("closure from callable", "Closure::fromCallable('nope')"),
    ("closure bind", "Closure::bind(fn() => 1, 1)"),
    ("closure bind to", "(fn() => 1)->bindTo(1)"),
    ("closure call", "(fn() => 1)->call(1)"),
    (
        "pdo query",
        "(new PDO('sqlite::memory:'))->query('select nope from nowhere')",
    ),
    (
        "inherited mysql connect",
        r"class_exists(Pdo\Mysql::class) ? Pdo\Mysql::connect() : null",
    ),
    (
        "inherited pgsql connect",
        r"class_exists(Pdo\Pgsql::class) ? Pdo\Pgsql::connect() : null",
    ),
    (
        "spl storage access",
        "(new SplObjectStorage())->offsetGet(new stdClass)",
    ),
    (
        "limit iterator offset",
        "new LimitIterator(new ArrayIterator([1]), -1)",
    ),
    (
        "reflection class constant",
        "new ReflectionClassConstant(stdClass::class, 'MISSING')",
    ),
    (
        "reflection method",
        "new ReflectionMethod(stdClass::class, 'missing')",
    ),
    (
        "reflection method name",
        "ReflectionMethod::createFromMethodName('no_colon')",
    ),
    (
        "reflection parameter",
        "new ReflectionParameter('strlen', 'missing')",
    ),
    (
        "reflection property",
        "new ReflectionProperty(stdClass::class, 'missing')",
    ),
    ("fiber cannot suspend", "Fiber::suspend('probe')"),
    (
        "generator cannot return",
        "(function () { yield 1; })()->getReturn()",
    ),
    ("invalid json depth", "json_decode('[]', depth: 0)"),
    ("invalid pad string", "str_pad('x', 3, '')"),
    ("invalid date", "(new DateTime())->modify('')"),
    (
        "invalid sqlite query",
        "(new SQLite3(':memory:'))->querySingle('invalid')",
    ),
    ("missing callback forwarded", "$callback('nope', [])"),
];

const RECEIVERS: &[(&str, &str)] = &[
    ("ArrayIterator", "new ArrayIterator([1])"),
    ("ArrayObject", "new ArrayObject([1])"),
    (
        "CachingIterator",
        "new CachingIterator(new ArrayIterator([1]))",
    ),
    ("DateTime", "new DateTime('2000-01-01')"),
    ("DateTimeImmutable", "new DateTimeImmutable('2000-01-01')"),
    ("DateTimeZone", "new DateTimeZone('UTC')"),
    ("DOMDocument", "new DOMDocument()"),
    ("Fiber", "new Fiber(fn() => 1)"),
    ("LimitIterator", "new LimitIterator(new ArrayIterator([1]))"),
    ("PDO", "new PDO('sqlite::memory:')"),
    (
        "PDOStatement",
        "(new PDO('sqlite::memory:'))->prepare('SELECT 1')",
    ),
    ("ReflectionClass", "new ReflectionClass(stdClass::class)"),
    ("ReflectionFunction", "new ReflectionFunction('strlen')"),
    (
        "ReflectionParameter",
        "new ReflectionParameter('strlen', 0)",
    ),
    ("SQLite3", "new SQLite3(':memory:')"),
    ("SplDoublyLinkedList", "new SplDoublyLinkedList()"),
    ("SplFixedArray", "new SplFixedArray(1)"),
    ("SplObjectStorage", "new SplObjectStorage()"),
    ("SplQueue", "new SplQueue()"),
    ("WeakMap", "new WeakMap()"),
];

#[tokio::test(flavor = "current_thread")]
async fn native_exception_frames_agree_with_php() {
    let runtime = Runtime::new();
    let mut source = PREAMBLE.to_owned();
    source.push_str("$callback = 'call_user_func_array';\n");
    for &(label, expression) in BODY_ERRORS {
        let _ = writeln!(source, "frameProbe('{label}', fn() => {expression});");
    }
    arity_probes(&mut source, &runtime.registry);
    let test_name = "native-exception-first-frames";
    let Some(php) = oracle::run_php(test_name, &source) else {
        return;
    };
    let script = oracle::script_path(test_name);
    let output = runtime.run_file(&script, &[], None).await.unwrap();
    assert_eq!(
        output.status,
        0,
        "{}",
        String::from_utf8_lossy(&output.output)
    );
    let output = String::from_utf8(output.output).unwrap();
    let observed: std::collections::HashMap<_, _> = output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let (labels, expected, actual) = compare_cases(&php, &observed);
    assert!(labels.len() >= 300, "only {} throwing probes", labels.len());
    oracle::compare("first native exception frames", &labels, &expected, &actual);
}

fn compare_cases(
    oracle: &[String],
    observed: &std::collections::HashMap<&str, &str>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut labels = Vec::new();
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in oracle {
        let Some((label, frame)) = line.split_once('\t') else {
            continue;
        };
        if !frame.starts_with("threw\t") {
            continue;
        }
        labels.push(label.to_owned());
        expected.push(frame.to_owned());
        actual.push(
            observed
                .get(label)
                .copied()
                .unwrap_or("missing row")
                .to_owned(),
        );
    }
    (labels, expected, actual)
}

fn arity_probes(source: &mut String, registry: &Registry) {
    let mut functions: Vec<_> = registry.functions().collect();
    functions.sort_unstable_by_key(|entry| entry.name);
    for entry in functions {
        if entry.min_args() == 0 {
            continue;
        }
        let name = entry.name.replace('\\', "\\\\");
        let _ = writeln!(
            source,
            "if (function_exists('{name}')) frameProbe('{name} without arguments', fn() => {}());",
            entry.name,
        );
    }
    let mut classes: Vec<_> = registry.classes().collect();
    classes.sort_unstable_by_key(|class| class.name);
    for class in classes {
        method_probes(source, class);
    }
}

fn method_probes(source: &mut String, class: &ClassEntry) {
    let receiver = RECEIVERS
        .iter()
        .find_map(|&(name, receiver)| (name == class.name).then_some(receiver));
    for method in class.methods {
        if method.visibility != Visibility::Public || method.params.iter().all(|p| p.optional) {
            continue;
        }
        let class_name = class.name;
        let name = method.name;
        let target = if method.is_static {
            format!("{class_name}::{name}")
        } else if let Some(receiver) = receiver {
            format!("({receiver})->{name}")
        } else {
            continue;
        };
        let class_name = class_name.replace('\\', "\\\\");
        let _ = writeln!(
            source,
            "if (method_exists('{class_name}', '{name}')) frameProbe('{class_name}::{name} without arguments', fn() => {target}());",
        );
    }
}
