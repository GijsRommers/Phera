//! libpq text results and parameter binding across PostgreSQL types.

use phera_extension_api::testing::{oracle, scripts::Comparison};

const TYPES: &[(&str, &str, &str)] = &[
    ("int2", "-32768", "32767"),
    ("int4", "-2147483648", "2147483647"),
    ("int8", "-9223372036854775808", "9223372036854775807"),
    ("oid", "0", "4294967295"),
    ("text", "🐾", ""),
    ("varchar(7)", " a ", "a'b"),
    ("bpchar(7)", "a", "xy"),
    ("bool", "false", "true"),
    ("numeric(26,6)", "-1234567891011123.123456", "0.001230"),
    ("float4", "-1.234567", "NaN"),
    ("float8", "-Infinity", "1.234567891234"),
    ("date", "2000-01-01", "2024-02-29"),
    ("time", "00:00:00.120000", "12:34:56.123456"),
    (
        "timestamp",
        "2001-01-01 00:00:00",
        "2024-02-29 12:34:56.123456",
    ),
    (
        "timestamptz",
        "2001-01-01 00:00:00+05:45",
        "2024-02-29 12:34:56.123456-03",
    ),
    ("interval", "2 years 3 days", "-1 hour 2 seconds"),
    ("bytea", r"\x00ff5c", r"\x"),
    ("int4[]", "{1,-2,NULL}", "{{1,2},{3,4}}"),
    ("text[]", r#"{"a,b",NULL,"NULL",""}"#, "{}"),
    ("json", r#"{"b":true,"a":null}"#, "[1,2]"),
    ("jsonb", r#"{"b":true,"a":null}"#, "[1,2]"),
    (
        "uuid",
        "00000000-0000-0000-0000-000000000000",
        "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
    ),
    ("inet", "127.0.0.1/24", "2001:db8::1"),
    ("bit(6)", "000000", "101010"),
];

const MATRIX: &str = r#"
function legacy_escape_cases($label) {
    foreach (['pg_escape_string', 'pg_escape_bytea', 'pg_escape_literal', 'pg_escape_identifier'] as $f) {
        run("$label-$f", fn() => $f("abc\x5c\x27\x22\x00\xff"));
    }
}
legacy_escape_cases('no-default');
$db = pg_connect('host=127.0.0.1 port=54329 dbname=pgsql_ext user=phera password=phera');
pg_query($db, "SET TIME ZONE 'UTC'");
legacy_escape_cases('default');
function matrix_read($r, $mode) {
    return match ($mode) {
        'assoc' => [pg_fetch_assoc($r), pg_fetch_assoc($r), pg_fetch_assoc($r), pg_fetch_assoc($r)],
        'row' => [pg_fetch_row($r, 2), pg_fetch_row($r, 1), pg_fetch_row($r)],
        'object' => [pg_fetch_object($r), pg_fetch_object($r, 2)],
        'cell' => [pg_fetch_result($r, 0, 'v'), pg_fetch_result($r, 1, 0), pg_fetch_result($r, 2, 'v')],
        'both' => pg_fetch_array($r, 0, PGSQL_BOTH),
        'array-assoc' => pg_fetch_array($r, 0, PGSQL_ASSOC),
        'num' => pg_fetch_array($r, 1, PGSQL_NUM),
        'all-num' => pg_fetch_all($r, PGSQL_NUM),
        'all-assoc' => pg_fetch_all($r, PGSQL_ASSOC),
        'all-both' => pg_fetch_all($r, PGSQL_BOTH),
        'all-columns' => pg_fetch_all_columns($r, 0),
        'fields' => [pg_num_rows($r), pg_num_fields($r), pg_field_name($r, 0),
            pg_field_type($r, 0), pg_field_type_oid($r, 0), pg_field_size($r, 0),
            pg_field_is_null($r, 2, 0), pg_field_prtlen($r, 0, 0), pg_field_num($r, 'v')],
    };
}
function matrix_submit($db, $simple, $params, $values, $statement, $protocol) {
    switch ($protocol) {
        case 'text': return pg_query($db, $simple);
        case 'params': return pg_query_params($db, $params, $values);
        case 'prepared': return pg_execute($db, $statement, $values);
        case 'send-text': pg_send_query($db, $simple); break;
        case 'send-params': pg_send_query_params($db, $params, $values); break;
        case 'send-execute': pg_send_execute($db, $statement, $values); break;
    }
    $result = pg_get_result($db);
    pg_get_result($db);
    return $result;
}
function matrix_type($db, $type, $a, $b, $id) {
    $simple = 'SELECT x.v FROM (VALUES (1, ' . pg_escape_literal($db, $a) . '::' . $type .
        '), (2, ' . pg_escape_literal($db, $b) . '::' . $type .
        '), (3, NULL::' . $type . ')) AS x(i, v) ORDER BY x.i';
    $params = 'SELECT x.v FROM (VALUES (1, $1::' . $type .
        '), (2, $2::' . $type . '), (3, NULL::' . $type . ')) AS x(i, v) ORDER BY x.i';
    $statement = 'matrix_type_' . $id;
    if ($id % 2 === 0) {
        pg_prepare($db, $statement, $params);
    } else {
        pg_send_prepare($db, $statement, $params);
        pg_get_result($db);
        pg_get_result($db);
    }
    foreach (['text', 'params', 'prepared', 'send-text', 'send-params', 'send-execute'] as $protocol) {
        foreach (['assoc', 'row', 'object', 'cell', 'both', 'num', 'array-assoc',
            'all-num', 'all-assoc', 'all-both', 'all-columns', 'fields'] as $mode) {
            run("$type-$protocol-$mode", function () use ($db, $simple, $params, $a, $b, $statement, $protocol, $mode) {
                $result = matrix_submit($db, $simple, $params, [$a, $b], $statement, $protocol);
                try { return matrix_read($result, $mode); }
                finally { pg_free_result($result); }
            });
        }
    }
}
function fixed_cases($db) {
    pg_query($db, 'BEGIN');
    pg_query($db, 'CREATE TABLE public.phera_oracle_type_matrix (id integer PRIMARY KEY, label text, value numeric(10,3), flag boolean DEFAULT true)');
    $table = 'public.phera_oracle_type_matrix';
    try {
        run('meta-data', fn() => pg_meta_data($db, $table));
        run('meta-data-extended', fn() => pg_meta_data($db, $table, true));
        run('convert', fn() => pg_convert($db, $table, ['id'=>'1', 'label'=>"a'b", 'value'=>'1.350', 'flag'=>false]));
        run('insert-sql', fn() => pg_insert($db, $table, ['id'=>'1', 'label'=>"a'b", 'value'=>'1.350'], PGSQL_DML_STRING));
        run('insert', fn() => (bool)pg_insert($db, $table, ['id'=>'1', 'label'=>"a'b", 'value'=>'1.350']));
        run('update-sql', fn() => pg_update($db, $table, ['flag'=>false], ['id'=>'1'], PGSQL_DML_STRING));
        run('update', fn() => (bool)pg_update($db, $table, ['flag'=>false], ['id'=>'1']));
        run('select-sql', fn() => pg_select($db, $table, ['id'=>'1'], PGSQL_DML_STRING));
        run('select', fn() => pg_select($db, $table, ['id'=>'1']));
        run('delete-sql', fn() => pg_delete($db, $table, ['id'=>'1'], PGSQL_DML_STRING));
        run('delete', fn() => (bool)pg_delete($db, $table, ['id'=>'1']));
        run('copy-from', fn() => pg_copy_from($db, $table, ["2|a|1.000|t\n", "3|NULL|2.800|f\n"], '|', 'NULL'));
        run('copy-to', fn() => pg_copy_to($db, $table, '|', 'NULL'));
    } finally {
        pg_query($db, 'ROLLBACK');
    }
    run('notice', function () use ($db) {
        pg_query($db, "DO \$\$ BEGIN RAISE NOTICE 'matrix notice'; END \$\$");
        $notice = pg_last_notice($db, PGSQL_NOTICE_LAST);
        $all = pg_last_notice($db, PGSQL_NOTICE_ALL);
        return [$notice, $all, pg_last_notice($db, PGSQL_NOTICE_CLEAR)];
    });
    run('query-null-param', fn() => pg_fetch_row(pg_query_params($db, 'SELECT $1::text, $2::bool', [null, true])));
    run('query-invalid-param', fn() => [pg_query_params($db, 'SELECT $1::int4', ['bad']), pg_last_error($db)]);
    run('async-query-error', function () use ($db) {
        pg_send_query($db, "SELECT CAST('bad' AS INTEGER)");
        $r = pg_get_result($db);
        return [pg_result_status($r), pg_result_error($r), pg_result_error_field($r, PGSQL_DIAG_SQLSTATE),
            pg_get_result($db)];
    });
    run('exec-alias', fn() => pg_fetch_row(pg_exec($db, 'SELECT 73')));
    foreach (['pg_numrows', 'pg_numfields', 'pg_cmdtuples', 'pg_getlastoid', 'pg_freeresult'] as $f) {
        run($f, fn() => $f(pg_query($db, 'SELECT 73 AS a')));
    }
    foreach (['pg_fieldname', 'pg_fieldsize', 'pg_fieldtype'] as $f) {
        run($f, fn() => $f(pg_query($db, 'SELECT 73 AS a'), 0));
    }
    run('pg_fieldnum', fn() => pg_fieldnum(pg_query($db, 'SELECT 73 AS a'), 'a'));
    run('pg_result', fn() => pg_result(pg_query($db, 'SELECT 73 AS a'), 0, 'a'));
    run('pg_result-legacy', fn() => pg_result(pg_query($db, 'SELECT 73 AS a'), 'a'));
    run('pg_errormessage', fn() => pg_errormessage($db));
    run('pg_setclientencoding', fn() => pg_setclientencoding($db, 'UTF8'));
    run('pg_clientencoding', fn() => pg_clientencoding($db));
}
"#;

#[test]
fn pgsql_php_matrix() {
    let Some(comparison) = Comparison::from_environment("pgsql-matrix") else {
        return;
    };
    let mut script = format!("<?php\n{}\n{MATRIX}\n", oracle::PHP_PREAMBLE);
    let literal = |value: &str| format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"));
    for (id, (kind, left, right)) in TYPES.iter().enumerate() {
        script.push_str(&format!(
            "matrix_type($db, {}, {}, {}, {id});\n",
            literal(kind),
            literal(left),
            literal(right),
        ));
    }
    script.push_str("fixed_cases($db);\npg_close($db);\n");
    comparison.check_matrix(&script, TYPES.len() * 72 + 40);
}
