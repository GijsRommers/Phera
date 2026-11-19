//! Compare pgsql's simple, prepared and asynchronous result paths with PHP.

use phera_extension_api::testing::{
    oracle,
    scripts::{Comparison, Runtime},
};

#[test]
fn function_declarations_match_php() {
    use std::fmt::Write;

    let Some(comparison) = Comparison::from_environment("pgsql-declarations") else {
        return;
    };
    let mut registry = phera_extension_api::Registry::new();
    registry.load(&phera_ext_pgsql::Pgsql);
    let mut script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
    for entry in registry.functions() {
        let name = entry.name;
        writeln!(
            script,
            "run('{name}', function () {{
                $r = new ReflectionFunction('{name}');
                $params = [];
                foreach ($r->getParameters() as $p) {{
                    $params[] = [$p->getName(), (string)$p->getType(),
                        $p->isOptional(), $p->isPassedByReference(), $p->isVariadic()];
                }}
                return [(string)$r->getReturnType(), $params];
            }});"
        )
        .expect("write declaration case");
    }
    comparison.check_matrix(&script, registry.functions().count());
}

const SETUP: &str = r#"
$db = pg_connect('host=127.0.0.1 port=54329 dbname=pgsql_ext user=phera password=phera');
function rows($db) {
    return pg_query($db, "SELECT 42::int4 AS i, TRUE AS b, 1.25::numeric AS d,
        '{a,b}'::text[] AS a, '2026-02-01'::date AS day, NULL AS n, '🐾' AS utf8");
}
"#;
const CALLS: &[(&str, &str)] = &[
    ("connection-status", "pg_connection_status($db)"),
    ("database", "pg_dbname($db)"),
    ("host", "pg_host($db)"),
    ("port", "pg_port($db)"),
    ("options", "pg_options($db)"),
    ("encoding", "pg_client_encoding($db)"),
    ("parameter", "pg_parameter_status($db, 'client_encoding')"),
    (
        "unknown-parameter",
        "pg_parameter_status($db, 'dsds_no_setting')",
    ),
    ("transaction", "pg_transaction_status($db)"),
    ("ping", "pg_ping($db)"),
    (
        "pid",
        "pg_get_pid($db) === (int)pg_fetch_result(pg_query($db, 'SELECT pg_backend_pid()'), 0, 0)",
    ),
    (
        "syntax-error",
        "(function () use ($db) {
        $value = pg_query($db, 'SELECT FROM'); return [$value, pg_last_error($db)]; })()",
    ),
    ("rows", "pg_num_rows(rows($db))"),
    ("fields", "pg_num_fields(rows($db))"),
    ("affected", "pg_affected_rows(rows($db))"),
    ("object", "pg_fetch_object(rows($db))"),
    ("assoc", "pg_fetch_assoc(rows($db))"),
    ("one-column", "pg_fetch_all_columns(rows($db), 2)"),
    ("field-name", "pg_field_name(rows($db), 2)"),
    ("field-type", "pg_field_type(rows($db), 2)"),
    ("field-type-oid", "pg_field_type_oid(rows($db), 2)"),
    ("field-size", "pg_field_size(rows($db), 0)"),
    ("field-table", "pg_field_table(rows($db), 0)"),
    ("field-number", "pg_field_num(rows($db), 'day')"),
    ("field-length", "pg_field_prtlen(rows($db), 0, 'utf8')"),
    ("field-null", "pg_field_is_null(rows($db), 0, 'n')"),
    ("fetch-cell", "pg_fetch_result(rows($db), 0, 'd')"),
    (
        "result-status",
        "pg_result_status(rows($db), PGSQL_STATUS_STRING)",
    ),
    ("result-error", "pg_result_error(rows($db))"),
    (
        "error-field",
        "pg_result_error_field(rows($db), PGSQL_DIAG_SQLSTATE)",
    ),
    ("result-oid", "pg_last_oid(rows($db))"),
    ("last-notice", "pg_last_notice($db, PGSQL_NOTICE_ALL)"),
    ("notify-idle", "pg_get_notify($db)"),
    (
        "escape-string",
        "pg_escape_string($db, \"Hello\\x27 \\\\ there\")",
    ),
    (
        "escape-literal",
        "pg_escape_literal($db, \"Hello\\x27 \\\\ there\")",
    ),
    (
        "escape-identifier",
        "pg_escape_identifier($db, \"a\\x22b\")",
    ),
    ("escape-bytea", "pg_escape_bytea($db, \"a\\x00\\xff\")"),
    ("unescape-bytea", "pg_unescape_bytea('\\x5c00ff')"),
    (
        "query-params",
        "pg_fetch_assoc(pg_query_params($db, 'SELECT $1::integer AS a, $2::boolean AS b', [1, true]))",
    ),
    (
        "prepare-execute",
        "(function () use ($db) { pg_prepare($db, 'q1', 'SELECT $1::text AS a');
        return pg_fetch_row(pg_execute($db, 'q1', ['hello'])); })()",
    ),
    (
        "send-query",
        "(function () use ($db) { $sent = pg_send_query($db, 'SELECT 1; SELECT 2');
        return [$sent, pg_fetch_row(pg_get_result($db)), pg_fetch_row(pg_get_result($db)),
            pg_get_result($db), pg_connection_busy($db)]; })()",
    ),
    (
        "positional-fetch-cursor",
        "(function () use ($db) { $q = pg_query($db, 'SELECT generate_series(1, 3) AS num');
            $explicit = pg_fetch_row($q, 1);
            $length = pg_field_prtlen($q, null, 0);
            return [$explicit, $length, pg_fetch_result($q, null, 'num'), pg_fetch_row($q)]; })()",
    ),
    (
        "poll-a-send",
        "(function () use ($db) { pg_send_query($db, 'SELECT 1 FROM pg_sleep(0.01)');
            $start = hrtime(true);
            while (pg_connection_busy($db) && hrtime(true) - $start < 2000000000) {
                pg_consume_input($db);
            }
            $busy = pg_connection_busy($db);
            return [$busy, pg_fetch_row(pg_get_result($db)), pg_get_result($db)]; })()",
    ),
    (
        "freed",
        "(function () use ($db) { $q = rows($db); pg_free_result($q);
        return pg_num_rows($q); })()",
    ),
];

#[test]
fn pgsql_matches_the_php_oracle() {
    let Some(comparison) = Comparison::from_environment("pgsql") else {
        return;
    };
    let mut script = format!("<?php\n{}\n{SETUP}\n", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    for (label, call) in CALLS {
        add(&mut script, &mut labels, label, call);
    }
    for mode in [1, 2, 3, 255] {
        for (label, expression) in [
            ("fetch-all", format!("pg_fetch_all(rows($db), {mode})")),
            (
                "fetch-array",
                format!("pg_fetch_array(rows($db), null, {mode})"),
            ),
        ] {
            add(
                &mut script,
                &mut labels,
                &format!("{label}-{mode}"),
                &expression,
            );
        }
    }
    let path = comparison.artifacts.join("calls.php");
    std::fs::write(&path, &script).expect("write comparison script");
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
    oracle::compare("pgsql", &labels, &lines(&php.stdout), &lines(&phera.stdout));
    assert!(php.status.success() && phera.status.success());
    assert_eq!(php.stderr, phera.stderr);
}

fn add(script: &mut String, labels: &mut Vec<String>, label: &str, expression: &str) {
    labels.push(label.into());
    script.push_str(&format!("run('{label}', fn() => {expression});\n"));
}
