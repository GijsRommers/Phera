//! Compare shipped PHP module claims with the installed PHP oracle.

use std::sync::Arc;

use phera_extension_api::testing::oracle;

#[tokio::test(flavor = "current_thread")]
async fn every_shipped_extension_reports_php_module_membership() {
    let runtime = super::Runtime::new();
    let mut modules: Vec<_> = runtime
        .registry
        .bundles()
        .chain(runtime.registry.extensions())
        .collect();
    modules.sort_unstable();
    modules.dedup();
    let entries = modules
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"<?php
foreach ([{entries}] as $name) {{
    $loaded = extension_loaded($name);
    echo $name, ':', (int) $loaded, ':', phpversion($name), PHP_EOL;
    echo (int) in_array(strtolower($name), array_map('strtolower', get_loaded_extensions()), true), PHP_EOL;
}}
"#
    );
    let Some(expected) = oracle::run_php("runtime-extension-introspection", &source) else {
        return;
    };
    let unit = phera_compiler::compile(&source, "extension-probe.php").unwrap();
    let mut vm = runtime.new_cli_vm();
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    let output = String::from_utf8(vm.output).unwrap();
    assert_eq!(output.lines().collect::<Vec<_>>(), expected);
}
