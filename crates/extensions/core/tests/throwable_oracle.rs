//! The exception classes compared with real PHP: what an exception prints
//! as a string and as a stack trace, for the same properties on both sides.
//!
//! PHP fills in file, line and trace itself, so the PHP side overwrites them
//! through reflection with the values the Rust side uses.

use std::rc::Rc;

use phera_ext_core::Core;
use phera_extension_api::testing::{TestContext, call, call_method, instantiate, oracle};
use phera_extension_api::{ArrayKey, Extension, Object, PhpArray, Registry, Value};

fn registry() -> Registry {
    let mut registry = Registry::new();
    Core.register(&mut registry);
    registry
}

fn map(entries: &[(&str, Value)]) -> Value {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(ArrayKey::str(*key), value.clone());
    }
    Value::from(array)
}

fn list(items: Vec<Value>) -> Value {
    let mut array = PhpArray::new();
    for item in items {
        array.push(item).expect("push");
    }
    Value::from(array)
}

/// A stack trace with every kind of frame and every kind of argument.
fn trace() -> Value {
    let arguments = list(vec![
        Value::Null,
        Value::Bool(true),
        Value::Bool(false),
        Value::Int(-7),
        Value::Float(1.5),
        Value::Float(0.1 + 0.2),
        Value::Float(1e25),
        Value::from("short"),
        Value::from("exactly15chars!"),
        Value::from("sixteen chars!!!"),
        Value::from("it's \"quoted\" and long enough to cut"),
        Value::from(""),
        list(vec![Value::Int(1)]),
    ]);
    list(vec![
        map(&[
            ("file", Value::from("/srv/app/lib.php")),
            ("line", Value::Int(12)),
            ("function", Value::from("helper")),
            ("args", arguments),
        ]),
        map(&[
            ("file", Value::from("/srv/app/Service.php")),
            ("line", Value::Int(40)),
            ("function", Value::from("run")),
            ("class", Value::from("App\\Service")),
            ("type", Value::from("->")),
            ("args", list(vec![])),
        ]),
        map(&[
            ("function", Value::from("array_map")),
            ("args", list(vec![Value::from("callback"), list(vec![])])),
        ]),
        map(&[
            ("file", Value::from("/srv/app/index.php")),
            ("line", Value::Int(3)),
            ("function", Value::from("boot")),
            ("class", Value::from("App\\Kernel")),
            ("type", Value::from("::")),
        ]),
    ])
}

struct Case {
    class: &'static str,
    message: &'static str,
    code: i64,
    file: &'static str,
    line: i64,
    trace: Value,
    /// The index of the case this one wraps, built earlier.
    previous: Option<usize>,
}

fn cases() -> Vec<Case> {
    let empty = || list(vec![]);
    vec![
        Case {
            class: "Exception",
            message: "plain",
            code: 0,
            file: "/srv/a.php",
            line: 1,
            trace: empty(),
            previous: None,
        },
        Case {
            class: "Exception",
            message: "",
            code: 5,
            file: "/srv/a.php",
            line: 2,
            trace: empty(),
            previous: None,
        },
        Case {
            class: "RuntimeException",
            message: "with a trace",
            code: 42,
            file: "/srv/app/lib.php",
            line: 12,
            trace: trace(),
            previous: None,
        },
        Case {
            class: "TypeError",
            message: "f(): Argument #1 must be of type int",
            code: 0,
            file: "/srv/b.php",
            line: 9,
            trace: empty(),
            previous: None,
        },
        Case {
            class: "LogicException",
            message: "outer",
            code: 1,
            file: "/srv/c.php",
            line: 30,
            trace: trace(),
            previous: Some(2),
        },
        Case {
            class: "Error",
            message: "outermost",
            code: 0,
            file: "/srv/d.php",
            line: 44,
            trace: empty(),
            previous: Some(4),
        },
        Case {
            class: "ErrorException",
            message: "multi\nline",
            code: 0,
            file: "/srv/e.php",
            line: 7,
            trace: empty(),
            previous: None,
        },
        Case {
            class: "DivisionByZeroError",
            message: "Division by zero",
            code: 0,
            file: "/srv/f.php",
            line: 8,
            trace: empty(),
            previous: None,
        },
    ]
}

/// The root class that declares the private properties of `class`.
fn root_of(registry: &Registry, class: &str) -> &'static str {
    let mut entry = registry.get_class(class).expect("class");
    while let Some(parent) = entry.parent {
        entry = registry.get_class(parent).expect("parent");
    }
    entry.name
}

#[test]
fn exceptions_print_like_php() {
    let registry = registry();
    let cases = cases();
    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);
    script.push_str(
        "function put($e, $class, $name, $value) { \
         $p = new ReflectionProperty($class, $name); $p->setValue($e, $value); }\n\
         function same_ids($f) { ob_start(); $r = $f(); \
         echo preg_replace('/\\)#\\d+ \\(/', ')#1 (', ob_get_clean()); return $r; }\n$E = [];\n",
    );

    let mut objects: Vec<Rc<Object>> = Vec::new();
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let root = root_of(&registry, case.class);
        let previous = case
            .previous
            .map_or("null".to_string(), |earlier| format!("$E[{earlier}]"));
        script.push_str(&format!(
            "$E[{index}] = new {}({}, {}{});\n\
             put($E[{index}], '{root}', 'file', {});\n\
             put($E[{index}], '{root}', 'line', {});\n\
             put($E[{index}], '{root}', 'trace', {});\n",
            case.class,
            oracle::php_literal(&Value::from(case.message)),
            case.code,
            // ErrorException's third parameter is the severity, not the
            // previous exception.
            if case.class == "ErrorException" {
                String::new()
            } else {
                format!(", {previous}")
            },
            oracle::php_literal(&Value::from(case.file)),
            case.line,
            oracle::php_literal(&case.trace),
        ));

        let object = instantiate(&registry, case.class).expect("instantiate");
        let mut context = TestContext::default();
        let mut arguments = vec![Value::from(case.message), Value::Int(case.code)];
        if let Some(earlier) = case.previous {
            arguments.push(Value::Object(objects[earlier].clone()));
        }
        call_method(&registry, &mut context, &object, "__construct", arguments).expect("construct");
        object.set(Object::protected_key("file"), Value::from(case.file));
        object.set(Object::protected_key("line"), Value::Int(case.line));
        object.set(Object::private_key(root, "trace"), case.trace.clone());

        for method in [
            "getMessage",
            "getCode",
            "getFile",
            "getLine",
            "getTrace",
            "getTraceAsString",
            "__toString",
        ] {
            script.push_str(&format!(
                "run('{method}', fn() => $E[{index}]->{method}());\n"
            ));
            labels.push(format!("{}#{index}->{method}()", case.class));
            let mut context = TestContext::default();
            let result = call_method(&registry, &mut context, &object, method, vec![]);
            rust.push(oracle::encode_case(
                method,
                &result,
                &context.output,
                &context.diagnostics,
            ));
        }
        // After __toString, so the cached text in the object is compared too.
        for function in ["var_dump", "print_r", "var_export"] {
            script.push_str(&format!(
                "run('{function}', fn() => same_ids(fn() => {function}($E[{index}])));\n"
            ));
            labels.push(format!("{function}({}#{index})", case.class));
            let mut context = TestContext::default();
            let result = call(
                &registry,
                &mut context,
                function,
                vec![Value::Object(object.clone())],
            );
            rust.push(oracle::encode_case(
                function,
                &result,
                &context.output,
                &context.diagnostics,
            ));
        }
        script.push_str(&format!(
            "run('previous', fn() => $E[{index}]->getPrevious() === {previous});\n"
        ));
        labels.push(format!("{}#{index}->getPrevious()", case.class));
        let mut context = TestContext::default();
        let found = call_method(&registry, &mut context, &object, "getPrevious", vec![]);
        let same = match (&found, case.previous) {
            (Ok(Value::Object(found)), Some(earlier)) => Rc::ptr_eq(found, &objects[earlier]),
            (Ok(Value::Null), None) => true,
            _ => false,
        };
        rust.push(format!("previous={}", oracle::encode(&Value::Bool(same))));
        objects.push(object);
    }

    let Some(php) = oracle::run_php("core-throwable", &script) else {
        eprintln!("php binary not found; skipping");
        return;
    };
    oracle::compare("exceptions", &labels, &php, &rust);
}

/// Every native class names a parent and interfaces that exist, and the
/// hierarchy matches PHP's own.
#[test]
fn the_hierarchy_matches_php() {
    let registry = registry();
    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for entry in registry.classes() {
        let name = entry.name;
        script.push_str(&format!(
            "run('{name}', fn() => [get_parent_class('{name}'), is_subclass_of('{name}', 'Throwable'), \
             (new ReflectionClass('{name}'))->isInterface()]);\n"
        ));
        labels.push(name.to_string());

        let mut ancestors = Vec::new();
        let mut next = Some(entry);
        while let Some(current) = next {
            ancestors.extend(current.interfaces.iter().copied());
            next = current.parent.and_then(|parent| registry.get_class(parent));
        }
        let parent = entry.parent.map_or(Value::Bool(false), Value::from);
        let throwable = ancestors.contains(&"Throwable");
        let is_interface = entry.kind == phera_extension_api::ClassKind::Interface;
        let summary = list(vec![
            parent,
            Value::Bool(throwable),
            Value::Bool(is_interface),
        ]);
        rust.push(format!("{name}={}", oracle::encode(&summary)));
    }
    let Some(php) = oracle::run_php("core-hierarchy", &script) else {
        eprintln!("php binary not found; skipping");
        return;
    };
    oracle::compare("class hierarchy", &labels, &php, &rust);
}
