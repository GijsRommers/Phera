//! The date extension is available through the real compiler and VM, per request.
use phera_extension_api::testing::oracle;
use phera_runtime::Runtime;

#[test]
fn procedural_date_functions_through_the_vm() {
    // Date calls return values and diagnostics; they do not print. The VM
    // has no output-buffer functions yet, so use the same encoder without buffering.
    let preamble = oracle::PHP_PREAMBLE
        .replace("ob_start();", "")
        .replace("$out = ob_get_clean();", "$out = '';")
        .replace(
            "bin2hex(pack('E', $f))",
            "json_encode($f, JSON_PRESERVE_ZERO_FRACTION)",
        );
    let script = format!(
        r#"<?php
{}
run('initial', fn()=>date_default_timezone_get());
foreach (['UTC', 'Europe/Brussels', 'America/New_York', 'Asia/Kathmandu',
          'Australia/Lord_Howe', 'Pacific/Apia'] as $zone) {{
    run('zone', fn()=>date_default_timezone_set($zone));
    foreach ([-2208988800, -1, 0, 1711848600, 1730613600, 4102444800] as $ts) {{
        run('date', fn()=>date('Y-m-d H:i:s.u e T Z I o-W-N c r', $ts));
        run('gmdate', fn()=>gmdate('Y-m-d H:i:s.u e T Z I o-W-N', $ts));
        run('getdate', fn()=>getdate($ts));
        run('localtime', fn()=>localtime($ts, true));
    }}
    run('mktime', fn()=>mktime(12, 34, 56, 2, 29, 2024));
    run('gmmktime', fn()=>gmmktime(25, -1, 61, 13, 0, 2024));
}}
run('parse', fn()=>date_parse_from_format('!Y-m-d H:i:s.u e',
    '2024-02-30 25:61:60.123456 Europe/Brussels'));
run('parse_nul', fn()=>date_parse_from_format('Y', "2024\0"));
run('idate', fn()=>idate('long', 0));
run('checkdate', fn()=>checkdate(2, 29, 1900));
"#,
        preamble
    );
    let php = oracle::run_php("date-runtime", &script).expect("PHP 8.5 oracle required");
    let path = std::env::temp_dir().join(format!("phera-date-runtime-{}.php", std::process::id()));
    std::fs::write(&path, script).unwrap();
    let runtime = Runtime::new();
    for _ in 0..2 {
        let result = runtime.run_file_blocking(&path, &[], None).unwrap();
        assert_eq!(
            result.status,
            0,
            "{}",
            String::from_utf8_lossy(&result.output)
        );
        let rust: Vec<_> = String::from_utf8(result.output)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        oracle::compare("date VM calls", &php, &php, &rust);
    }
    std::fs::remove_file(path).unwrap();
}
