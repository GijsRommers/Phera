//! Exercise the registered native methods with the same coercion as the CLI.

use std::{any::Any, fmt::Write, rc::Rc};

use phera_ext_date::Date;
use phera_extension_api::{
    Context, LocalFuture, NativeMethod, NativeResult, Object, PhpArray, Registry, RequestState,
    Value, prepare_call,
    testing::{TestContext, oracle},
    value::{ClassHandle, Diagnostics},
};

pub const FORMAT: &str = r"Y-m-d\TH:i:s.uP e I";
pub const INTERVAL_FORMAT: &str = "%r%Y-%M-%D %H:%I:%S.%F | %a | %R %y %m %d %h %i %s %f %% %z %";

pub struct Native {
    registry: Registry,
    pub context: TestContext,
}

impl Native {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.load(&phera_ext_core::Core);
        registry.load(&Date);
        Self {
            registry,
            context: TestContext::default(),
        }
    }

    pub fn call(
        &mut self,
        class: &str,
        this: Option<&Rc<Object>>,
        method: &str,
        mut arguments: Vec<Value>,
    ) -> NativeResult {
        let entry = self
            .registry
            .get_class(class)
            .expect("class")
            .methods
            .iter()
            .find(|entry| entry.name == method)
            .expect("method");
        let NativeMethod::Sync(handler) = entry.handler else {
            panic!("sync method");
        };
        let site = format!("{class}::{method}");
        let mut context = DateContext {
            base: &mut self.context,
        };
        prepare_call(&site, entry.params, &mut arguments, &mut context)?;
        handler(&mut context, this, &arguments)
    }

    pub fn new_object(&mut self, class: &str, arguments: Vec<Value>) -> NativeResult {
        let object = object(class, PhpArray::new());
        self.call(class, Some(&object), "__construct", arguments)?;
        Ok(Value::Object(object))
    }

    pub fn method(&mut self, value: &Value, method: &str, arguments: Vec<Value>) -> NativeResult {
        let Value::Object(object) = value else {
            panic!("method receiver");
        };
        self.call(object.class.name(), Some(object), method, arguments)
    }

    pub fn date_output(&mut self, result: NativeResult) -> NativeResult {
        result.and_then(|date| {
            if let Value::Object(_) = date {
                self.method(&date, "format", vec![FORMAT.into()])
            } else {
                Ok(date)
            }
        })
    }

    pub fn interval_output(&mut self, result: NativeResult) -> NativeResult {
        result.and_then(|interval| self.method(&interval, "format", vec![INTERVAL_FORMAT.into()]))
    }
}

struct DateContext<'a> {
    base: &'a mut TestContext,
}

impl Context for DateContext<'_> {
    fn write(&mut self, bytes: &[u8]) {
        self.base.write(bytes);
    }
    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self.base.diagnostics()
    }
    fn state(&mut self) -> &mut RequestState {
        self.base.state()
    }
    fn strict_types(&self) -> bool {
        self.base.strict_types()
    }
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        args: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        self.base.call(callable, args)
    }
    fn create_object(&mut self, class: &str, properties: PhpArray) -> NativeResult {
        Ok(Value::Object(object(class, properties)))
    }
}

struct DateClass(String);

impl ClassHandle for DateClass {
    fn name(&self) -> &str {
        &self.0
    }
    fn is_instance_of(&self, name: &str) -> bool {
        self.0.eq_ignore_ascii_case(name)
            || (name == "DateTimeInterface"
                && matches!(self.0.as_str(), "DateTime" | "DateTimeImmutable"))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn object(class: &str, properties: PhpArray) -> Rc<Object> {
    let object = Object::new(1, Rc::new(DateClass(class.into())));
    object.update(|stored| *stored = properties);
    Rc::new(object)
}

pub struct Matrix {
    pub native: Native,
    script: String,
    labels: Vec<String>,
    actual: Vec<String>,
}

impl Matrix {
    pub fn new() -> Self {
        let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
        writeln!(script, "const DATETIME_FORMAT = '{FORMAT}';").unwrap();
        writeln!(script, "const INTERVAL_FORMAT = '{INTERVAL_FORMAT}';").unwrap();
        script.push_str(
            r#"
function date_value($date) { return $date === false ? false : $date->format(DATETIME_FORMAT); }
function packed($date) { return [date_value($date), DateTimeImmutable::getLastErrors()]; }
"#,
        );
        Self {
            native: Native::new(),
            script,
            labels: Vec::new(),
            actual: Vec::new(),
        }
    }

    pub fn setup(&mut self, statement: &str) {
        writeln!(self.script, "{statement}").unwrap();
    }

    pub fn case(&mut self, label: String, expression: &str, result: NativeResult) {
        writeln!(self.script, "run('c', fn()=>{expression});").unwrap();
        self.actual.push(oracle::encode_case(
            "c",
            &result,
            &self.native.context.output,
            &self.native.context.diagnostics,
        ));
        self.native.context.output.clear();
        self.native.context.diagnostics.entries.clear();
        self.labels.push(label);
    }

    pub fn finish(self, name: &str) {
        let expected = oracle::run_php(name, &self.script).expect("PHP 8.5 oracle");
        let mismatches: Vec<_> = self
            .labels
            .iter()
            .zip(expected.iter().zip(&self.actual))
            .filter(|(_, (php, rust))| php != rust)
            .map(|(label, (php, rust))| format!("{label}\tPHP={php}\tPhera={rust}"))
            .collect();
        let path = std::env::temp_dir().join(format!("{name}-mismatches.txt"));
        std::fs::write(path, mismatches.join("\n")).expect("mismatch evidence");
        oracle::compare(name, &self.labels, &expected, &self.actual);
    }
}
