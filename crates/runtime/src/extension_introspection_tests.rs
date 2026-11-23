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
    let compare_php_version = oracle::php_version_id().is_some()
        && oracle::supports_php_since("versions of modules bundled with PHP", 80_513);
    let compare_php_version = if compare_php_version { "true" } else { "false" };
    let source = format!(
        r#"<?php
foreach ([{entries}] as $name) {{
    $loaded = extension_loaded($name);
    $version = phpversion($name);
    echo $name, ':', (int) $loaded, ':', {compare_php_version} || $version !== PHP_VERSION
        ? $version : 'bundled', PHP_EOL;
    echo (int) in_array(strtolower($name), array_map('strtolower', get_loaded_extensions()), true), PHP_EOL;
}}
foreach ([
    'FiLeInFo' => ['finfo_open', 'finfo_close', 'finfo_set_flags', 'finfo_file',
                   'finfo_buffer', 'mime_content_type'],
    'gD' => ['gd_info', 'imagecreatetruecolor', 'imagepng', 'imagesx', 'imagettftext'],
    'eXiF' => ['exif_imagetype'],
    'standard' => ['getimagesize', 'getimagesizefromstring', 'image_type_to_mime_type',
                   'image_type_to_extension'],
] as $module => $sample) {{
    $functions = get_extension_funcs($module);
    $selected = $functions === false ? false : array_values(array_intersect($functions, $sample));
    echo json_encode($selected), PHP_EOL;
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
