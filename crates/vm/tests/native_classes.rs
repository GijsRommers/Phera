//! Native registration must feed the same constant and Throwable paths as PHP declarations.

use std::sync::Arc;

use phera_ext_core::Core;
use phera_extension_api::{
    ClassEntry, ConstantValue, Context, Extension, FunctionEntry, LocalFuture, MethodEntry,
    NativeError, NativeResult, PhpArray, PropertyEntry, Registry, Value, Visibility,
};
use phera_vm::Vm;

fn failure(_context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    let mut information = PhpArray::new();
    information.push(Value::from("42P01")).unwrap();
    information.push(Value::Int(7)).unwrap();
    information.push(Value::from("missing")).unwrap();
    Err(NativeError::throw("DriverException", "missing")
        .with_code("42P01")
        .with_property("errorInfo", Value::from(information)))
}

fn bad_property(_context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    Err(NativeError::throw("DriverException", "missing").with_property("notDeclared", Value::Null))
}

fn asynchronous_failure<'context>(
    context: &'context mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'context, NativeResult> {
    Box::pin(async move { failure(context, &arguments) })
}

fn registry() -> Registry {
    const PROPERTIES: &[PropertyEntry] = &[PropertyEntry::new(
        "errorInfo",
        ConstantValue::Null,
        Visibility::Public,
    )];
    let mut registry = Registry::new();
    Core.register(&mut registry);
    registry.class(ClassEntry {
        parent: Some("Exception"),
        properties: PROPERTIES,
        ..ClassEntry::class("DriverException")
    });
    registry.function(FunctionEntry::sync("native_failure", &[], failure));
    registry.function(FunctionEntry::sync("bad_property", &[], bad_property));
    registry.function(FunctionEntry::asynchronous(
        "async_failure",
        &[],
        asynchronous_failure,
    ));
    registry
}

async fn run(registry: Registry, source: &str) -> (i32, String) {
    let unit = phera_compiler::compile(source, "native.php").unwrap();
    let mut vm = Vm::new(Arc::new(registry));
    let status = vm.run(Arc::new(unit)).await;
    (status, String::from_utf8(vm.output.clone()).unwrap())
}

/// The call-site snapshot and real core getters/dumper must see the payload before PHP catches it.
#[tokio::test(flavor = "current_thread")]
async fn caught_native_exception_has_string_code_properties_and_call_site() {
    let source = r#"<?php
try {
    native_failure();
} catch (DriverException $error) {
    var_dump($error->getCode(), $error->errorInfo, $error->getPrevious());
    echo $error->getFile(), ':', $error->getLine(), "\n";
    echo $error->getTraceAsString(), "\n";
    echo (string) $error, "\n";
    var_dump($error);
}
"#;
    let (status, output) = run(registry(), source).await;
    assert_eq!(status, 0, "{output}");
    assert!(
        output.starts_with(concat!(
            "string(5) \"42P01\"\narray(3) {\n",
            "  [0]=>\n  string(5) \"42P01\"\n",
            "  [1]=>\n  int(7)\n  [2]=>\n  string(7) \"missing\"\n}\nNULL\n",
            "native.php:3\n#0 native.php(3): native_failure()\n#1 {main}\n",
            "DriverException: missing in native.php:3\nStack trace:\n",
            "#0 native.php(3): native_failure()\n#1 {main}\n",
        )),
        "{output}"
    );
    assert!(
        output.contains("[\"code\":protected]=>\n  string(5) \"42P01\""),
        "{output}"
    );
    assert!(
        output.contains("[\"errorInfo\"]=>\n  array(3) {"),
        "{output}"
    );
}

/// Uncaught reporting and callback boundaries must materialize the same rich native request.
#[tokio::test(flavor = "current_thread")]
async fn uncaught_native_exception_renders_the_real_php_format() {
    let (status, output) = run(registry(), "<?php\nnative_failure();\n").await;
    assert_eq!(status, 255);
    assert_eq!(
        output,
        concat!(
            "\nFatal error: Uncaught DriverException: missing in native.php:2\n",
            "Stack trace:\n#0 native.php(2): native_failure()\n#1 {main}\n",
            "  thrown in native.php on line 2\n",
        )
    );
}

/// Asynchronous native functions use the same cold materialization path.
#[tokio::test(flavor = "current_thread")]
async fn asynchronous_native_exception_keeps_its_payload() {
    let (status, output) = run(
        registry(),
        r#"<?php
try { async_failure(); } catch (DriverException $error) {
    var_dump($error->getCode(), $error->errorInfo[0]);
}
"#,
    )
    .await;
    assert_eq!(status, 0, "{output}");
    assert_eq!(output, "string(5) \"42P01\"\nstring(5) \"42P01\"\n");
}

/// A bad extension property is reported clearly instead of becoming a PHP dynamic property.
#[tokio::test(flavor = "current_thread")]
async fn undeclared_native_exception_property_is_an_internal_error() {
    let (status, output) = run(
        registry(),
        r#"<?php
try { bad_property(); } catch (Error $error) { echo $error->getMessage(); }
"#,
    )
    .await;
    assert_eq!(status, 0, "{output}");
    assert_eq!(
        output,
        concat!(
            "Phera internal error: native exception property ",
            "DriverException::$notDeclared is not declared",
        )
    );
}

/// Native interface values must propagate through native and user inheritance, including diamonds.
#[tokio::test(flavor = "current_thread")]
async fn native_interface_constants_use_ordinary_inheritance_and_constant_expressions() {
    const CONSTANTS: &[(&str, ConstantValue)] = &[
        ("MODE", ConstantValue::Int(7)),
        ("LABEL", ConstantValue::String("native")),
        ("ENABLED", ConstantValue::Bool(true)),
        ("RATIO", ConstantValue::Float(1.5)),
        ("NOTHING", ConstantValue::Null),
        ("ITEMS", ConstantValue::EmptyArray),
    ];
    let mut registry = registry();
    registry.class(ClassEntry {
        constants: CONSTANTS,
        ..ClassEntry::interface("NativeModes")
    });
    registry.class(ClassEntry {
        interfaces: &["NativeModes"],
        ..ClassEntry::interface("NativeChildModes")
    });
    registry.class(ClassEntry {
        interfaces: &["NativeChildModes"],
        ..ClassEntry::class("NativeBase")
    });
    registry.class(ClassEntry {
        parent: Some("NativeBase"),
        ..ClassEntry::class("NativeChild")
    });
    let source = r#"<?php
interface UserModes extends NativeModes {}
class Child extends NativeChild implements UserModes {
    public const int MODE=9;
    public $mode=parent::MODE;
    public static function mode($mode=self::MODE) {
        return [$mode, parent::MODE, static::MODE];
    }
}
const DEFAULT_MODE=NativeModes::MODE;
var_dump(NativeModes::MODE, NativeChildModes::LABEL, NativeChild::MODE, DEFAULT_MODE);
var_dump((new Child)->mode, Child::mode());
var_dump(NativeModes::ENABLED, NativeModes::RATIO, NativeModes::NOTHING, NativeModes::ITEMS);
var_dump(constant('Child::LABEL'), defined('NativeChild::MODE'), defined('NativeModes::NOPE'));
"#;
    let (status, output) = run(registry, source).await;
    assert_eq!(status, 0, "{output}");
    assert_eq!(
        output,
        concat!(
            "int(7)\nstring(6) \"native\"\nint(7)\nint(7)\nint(7)\n",
            "array(3) {\n  [0]=>\n  int(9)\n  [1]=>\n  int(7)\n  [2]=>\n  int(9)\n}\n",
            "bool(true)\nfloat(1.5)\nNULL\narray(0) {\n}\n",
            "string(6) \"native\"\nbool(true)\nbool(false)\n",
        )
    );
}

/// Native methods use the same exception payload contract as native functions.
#[tokio::test(flavor = "current_thread")]
async fn native_method_exception_keeps_its_payload() {
    fn method_failure(
        context: &mut dyn Context,
        _object: Option<&std::rc::Rc<phera_extension_api::Object>>,
        arguments: &[Value],
    ) -> NativeResult {
        failure(context, arguments)
    }
    const METHODS: &[MethodEntry] = &[MethodEntry::public("fail", &[], method_failure)];
    let mut registry = registry();
    registry.class(ClassEntry {
        methods: METHODS,
        ..ClassEntry::class("Driver")
    });
    let (status, output) = run(
        registry,
        r#"<?php
try { (new Driver)->fail(); } catch (DriverException $error) {
    var_dump($error->getCode(), $error->errorInfo[0]);
}
"#,
    )
    .await;
    assert_eq!(status, 0, "{output}");
    assert_eq!(output, "string(5) \"42P01\"\nstring(5) \"42P01\"\n");
}

/// Wrapping a native failure preserves its string code and identity in getPrevious() chains.
#[tokio::test(flavor = "current_thread")]
async fn previous_chain_keeps_the_native_exception_object() {
    let (status, output) = run(
        registry(),
        r#"<?php
try {
    native_failure();
} catch (DriverException $error) {
    $wrapped=new Exception('wrapper', 17, $error);
    var_dump($wrapped->getPrevious() === $error);
    var_dump($wrapped->getPrevious()->getCode(), $wrapped->getPrevious()->errorInfo[0]);
    echo (string)$wrapped;
}
"#,
    )
    .await;
    assert_eq!(status, 0, "{output}");
    assert_eq!(
        output,
        concat!(
            "bool(true)\nstring(5) \"42P01\"\nstring(5) \"42P01\"\n",
            "DriverException: missing in native.php:3\nStack trace:\n",
            "#0 native.php(3): native_failure()\n#1 {main}\n\n",
            "Next Exception: wrapper in native.php:5\nStack trace:\n#0 {main}",
        )
    );
}
