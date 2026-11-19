//! Check the parameter data exposed to PHP, including `round()\'s` enum default.

use std::fmt::Write;
use std::sync::Arc;

use phera_extension_api::Registry;
use phera_extension_api::testing::oracle;

use super::super::Runtime;

const DISCOVER: &str = r#"<?php
foreach (get_defined_functions()['internal'] as $name) echo $name, "\n";
foreach (array_merge(get_declared_classes(), get_declared_interfaces(), get_declared_traits()) as $name) {
    foreach ((new ReflectionClass($name))->getMethods() as $method) {
        if ($method->getDeclaringClass()->getName() === $name) {
            echo $name, '::', $method->getName(), "\n";
        }
    }
}
"#;

const INSPECT: &str = r#"
function inspectDefaults(array $symbols): void {
    foreach ($symbols as $symbol) {
        if (str_contains($symbol, '::')) {
            [$class, $method] = explode('::', $symbol);
            if (!method_exists($class, $method)) continue;
            $reflection = new ReflectionMethod($class, $method);
        } else {
            if (!function_exists($symbol)) continue;
            $reflection = new ReflectionFunction($symbol);
        }
        $record = [];
        foreach ($reflection->getParameters() as $parameter) {
            $available = $parameter->isDefaultValueAvailable();
            $value = $available ? $parameter->getDefaultValue() : null;
            if ($value instanceof UnitEnum) $value = $value::class . '::' . $value->name;
            $record[] = [
                $parameter->getName(),
                $parameter->isOptional(),
                $parameter->isPassedByReference(),
                $parameter->isVariadic(),
                $available,
                $value,
                $available ? $parameter->getDefaultValueConstantName() : null,
            ];
        }
        echo $symbol, "\t", json_encode($record, JSON_PRESERVE_ZERO_FRACTION), "\n";
    }
}
"#;

#[tokio::test(flavor = "current_thread")]
async fn reflected_native_parameters_agree_with_php() {
    let runtime = Runtime::new();
    let mut symbols = symbols(&runtime.web_registry);
    let discovered = run_reflection_script(&runtime, DISCOVER).await;
    symbols.extend(discovered.lines().map(str::to_owned));
    symbols.sort_unstable();
    symbols.dedup();
    let mut source = String::from("<?php\n");
    source.push_str(INSPECT);
    invoke(&mut source, &symbols);
    let php = oracle::run_php("native-parameter-reflection", &source).expect("PHP oracle required");
    symbols = php
        .iter()
        .map(|line| {
            line.split_once('\t')
                .expect("oracle parameter record")
                .0
                .to_owned()
        })
        .collect();
    let mut source = String::from("<?php\n");
    source.push_str(INSPECT);
    invoke(&mut source, &symbols);
    let output = run_reflection_script(&runtime, &source).await;
    let output: Vec<_> = output.lines().map(str::to_owned).collect();
    oracle::compare("reflected native parameters", &symbols, &php, &output);
}

async fn run_reflection_script(runtime: &Runtime, source: &str) -> String {
    let unit = phera_compiler::compile(source, "native-parameters.php").unwrap();
    let mut vm = runtime.new_cli_vm();
    assert_eq!(
        vm.run(Arc::new(unit)).await,
        0,
        "{}",
        String::from_utf8_lossy(&vm.output)
    );
    String::from_utf8(vm.output).unwrap()
}

fn symbols(registry: &Registry) -> Vec<String> {
    registry
        .functions()
        .map(|function| function.name.to_owned())
        .chain(registry.classes().flat_map(|class| {
            class
                .methods
                .iter()
                .map(|method| format!("{}::{}", class.name, method.name))
        }))
        .collect()
}

fn invoke(source: &mut String, symbols: &[String]) {
    source.push_str("\ninspectDefaults([\n");
    for symbol in symbols {
        let _ = writeln!(source, "'{}',", symbol.replace('\\', "\\\\"));
    }
    source.push_str("]);\n");
}
