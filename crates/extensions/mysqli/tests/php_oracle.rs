//! Compare mysqli's procedural and object calls with PHP's mysqlnd.

use phera_extension_api::testing::{
    oracle,
    scripts::{Comparison, Runtime},
};

const SETUP: &str = r#"
$db = mysqli_init();
mysqli_real_connect($db, '127.0.0.1', 'phera', 'phera', 'mysqli_ext', 33069);
$db->set_charset('utf8mb4');
function rows($db) {
    return mysqli_query($db, "SELECT 42 AS i, 1e0 AS f, CAST(1.25 AS DECIMAL(5,2)) AS d,
        CAST('2026-02-01' AS DATE) AS day, NULL AS n, '🐾' AS utf8, 0xFF AS binary_value");
}
"#;
const CALLS: &[(&str, &str)] = &[
    ("default-report", "(new mysqli_driver())->report_mode"),
    ("host-info", "mysqli_get_host_info($db)"),
    ("charset", "$db->character_set_name()"),
    ("sqlstate", "mysqli_sqlstate($db)"),
    ("error-list", "$db->error_list"),
    ("procedural-column", "mysqli_fetch_column(rows($db), 1)"),
    ("object-column", "rows($db)->fetch_column(2)"),
    ("object-row", "rows($db)->fetch_row()"),
    ("procedural-assoc", "mysqli_fetch_assoc(rows($db))"),
    ("object-default", "rows($db)->fetch_array()"),
    ("object-instance", "rows($db)->fetch_object()"),
    ("num-rows", "mysqli_num_rows(rows($db))"),
    ("num-fields", "mysqli_num_fields(rows($db))"),
    ("buffered-seek", "rows($db)->data_seek(0)"),
    (
        "seek-beyond-end",
        "mysqli_data_seek(rows($db), PHP_INT_MAX)",
    ),
    ("negative-seek", "rows($db)->data_seek(-1)"),
    ("invalid-field", "mysqli_fetch_field_direct(rows($db), 72)"),
    ("invalid-mode", "rows($db)->fetch_array(-1)"),
    (
        "prepare-direct",
        "$db->execute_query('SELECT ? + 0 AS n', [42])->fetch_row()",
    ),
    ("wrong-params", "$db->execute_query('SELECT ?', [])"),
    ("escape-empty", "mysqli_real_escape_string($db, '')"),
    (
        "escape-bytes",
        "mysqli_real_escape_string($db, \"\\x00\\n\\r\\x1a\\xff\\\\'\\\"\")",
    ),
    ("empty-query", "mysqli_query($db, '')"),
    (
        "close",
        "(function () use ($db) { $q=rows($db); $q->free(); return $q->fetch_row(); })()",
    ),
];

#[test]
fn mysqli_matches_the_php_oracle() {
    let Some(comparison) = Comparison::from_environment("mysqli") else {
        return;
    };
    let mut script = format!("<?php\n{}\n{SETUP}\n", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    for (label, expression) in CALLS {
        add(&mut script, &mut labels, label, expression);
    }
    if oracle::supports_php_since("mysqlnd patch-version metadata", 80513) {
        add(
            &mut script,
            &mut labels,
            "client-info",
            "mysqli_get_client_info()",
        );
        add(
            &mut script,
            &mut labels,
            "client-version",
            "mysqli_get_client_version()",
        );
    }
    for mode in [1, 2, 3] {
        for (label, expression) in [
            ("all", format!("rows($db)->fetch_all({mode})")),
            ("array", format!("mysqli_fetch_array(rows($db), {mode})")),
        ] {
            add(
                &mut script,
                &mut labels,
                &format!("{label}-{mode}"),
                &expression,
            );
        }
    }
    for mode in [0, 1, 2, 3, 255] {
        add(
            &mut script,
            &mut labels,
            &format!("report-{mode}"),
            &format!(
                "(function () use ($db) {{ mysqli_report({mode});
                try {{ return $db->query('SELECT FROM'); }}
                catch (mysqli_sql_exception $e) {{ return [$e->getCode(), $e->getSqlState()]; }}
                finally {{ mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT); }} }})()"
            ),
        );
    }
    add(
        &mut script,
        &mut labels,
        "bind-ref",
        "(function () use ($db) { $stmt = $db->prepare('SELECT ? AS value');
            $v = 5; $stmt->bind_param('i', $v); $v = 6; $stmt->execute();
            $stmt->bind_result($out); $stmt->fetch(); return $out; })()",
    );
    add(
        &mut script,
        &mut labels,
        "real-query",
        "(function () use ($db) { mysqli_real_query($db, 'SELECT 1');
           return mysqli_store_result($db)->fetch_row(); })()",
    );
    add(
        &mut script,
        &mut labels,
        "multi-query",
        "(function () use ($db) { $db->multi_query('SELECT 1; SELECT 2');
           $first=$db->store_result()->fetch_row(); $more=$db->more_results();
           $next=$db->next_result(); return [$first, $more, $next,
               $db->store_result()->fetch_row(), $db->more_results()]; })()",
    );
    add(
        &mut script,
        &mut labels,
        "iteration",
        "(function () use ($db) { $out=[]; foreach (rows($db) as $key=>$row) {
            $out[$key]=$row; } return $out; })()",
    );
    compare(&comparison, &script, &labels);
}

fn add(script: &mut String, labels: &mut Vec<String>, label: &str, expression: &str) {
    labels.push(label.into());
    script.push_str(&format!("run('{label}', fn() => {expression});\n"));
}

fn compare(comparison: &Comparison, script: &str, labels: &[String]) {
    let path = comparison.artifacts.join("calls.php");
    std::fs::write(&path, script).expect("write comparison script");
    let php = comparison
        .command(Runtime::Php, &path)
        .output()
        .expect("PHP oracle");
    let phera = comparison
        .command(Runtime::Phera, &path)
        .output()
        .expect("Phera");
    let lines = |data: &[u8]| {
        String::from_utf8_lossy(data)
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let _ = comparison.check("calls", &php, &phera);
    oracle::compare("mysqli", labels, &lines(&php.stdout), &lines(&phera.stdout));
    assert!(php.status.success() && phera.status.success());
    assert_eq!(php.stderr, phera.stderr);
}
