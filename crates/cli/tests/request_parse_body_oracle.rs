//! Validate the function signature and every option value against PHP's CLI.

use std::process::Command;

use phera_extension_api::testing::oracle;

#[test]
fn cli_request_parse_body_signatures_and_options_match_php() {
    let script = [
        "<?php\n",
        oracle::PHP_PREAMBLE,
        r#"
$keys = ['post_max_size', 'max_input_vars', 'max_multipart_body_parts', 'max_file_uploads',
    'upload_max_filesize', 'POST_MAX_SIZE', 'Max_Input_Vars', 'MAX_MULTIPART_BODY_PARTS',
    'MAX_FILE_UPLOADS', 'Upload_Max_Filesize', 'unknown', '', 42, '42', "max_input_vars\0",
    "unknown\0trailing"];
$values = [null, true, false, 0, -1, 1, PHP_INT_MIN, PHP_INT_MAX, 1.5, INF, NAN,
    [], ['nested'], new stdClass, STDIN, '', ' ', '0', '-0', '+0', ' 4K ', '2m', '-1G',
    '32', '1GB', 'x', '1n', '1.5M', '18446744073709551616', "1\0", "\xff"];
foreach (range(-4, 4) as $value) {
    foreach (['', 'k', 'M', 'G', 'KiB', 'e1', ';'] as $suffix) {
        $values[] = "$value$suffix";
    }
}
$call = function ($options) {
    try {
        return request_parse_body(options: $options);
    } catch (Throwable $e) {
        return [get_class($e), $e->getMessage()];
    }
};
foreach ($keys as $i => $key) {
    foreach ($values as $j => $value) {
        $byRef = [&$value];
        run("$i/$j", fn () => $call([$key => $byRef[0]]));
    }
}
foreach ($values as $j => $value) run("argument/$j", fn () => $call($value));
run('omitted', fn () => request_parse_body());
run('extra', fn () => request_parse_body(null, null));
run('unknown-name', fn () => request_parse_body(other: null));
run('alias', function () {
    $value = '1M';
    $options = ['post_max_size' => &$value, 'POST_MAX_SIZE' => 1];
    return [$GLOBALS['call']($options), $value];
});
run('quantity-then-invalid-key', fn () => $call(['post_max_size' => '7GB', 42 => null]));
run('invalid-key-then-quantity', fn () => $call([42 => null, 'post_max_size' => '7GB']));
"#,
    ]
    .concat();
    let name = "request-parse-body-cli";
    let expected = oracle::run_php(name, &script).expect("PHP 8.5 required");
    let output = Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg(oracle::script_path(name))
        .output()
        .expect("run the Phera script");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    let actual: Vec<_> = stdout.lines().map(str::to_string).collect();
    assert_eq!(actual.len(), 1604);
    oracle::compare("request_parse_body CLI", &[], &expected, &actual);
}
