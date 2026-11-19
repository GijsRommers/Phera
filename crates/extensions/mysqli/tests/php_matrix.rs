//! mysqlnd row conversion across binary and text protocols.

use phera_extension_api::testing::{oracle, scripts::Comparison};

const COLUMNS: &[(&str, &str, &str, &str)] = &[
    ("tiny", "TINYINT", "-128", "127"),
    ("utiny", "TINYINT UNSIGNED", "0", "255"),
    ("small", "SMALLINT", "-32768", "32767"),
    ("usmall", "SMALLINT UNSIGNED", "0", "65535"),
    ("medium", "MEDIUMINT", "-8388608", "8388607"),
    ("umedium", "MEDIUMINT UNSIGNED", "0", "16777215"),
    ("integer", "INT", "-2147483648", "2147483647"),
    ("uinteger", "INT UNSIGNED", "0", "4294967295"),
    (
        "big",
        "BIGINT",
        "-9223372036854775808",
        "9223372036854775807",
    ),
    (
        "ubig",
        "BIGINT UNSIGNED",
        "9223372036854775808",
        "18446744073709551615",
    ),
    (
        "decimal",
        "DECIMAL(24,6)",
        "-1234567891011123.123456",
        "0.001230",
    ),
    ("float", "FLOAT", "-1.234567", "1.25"),
    ("double", "DOUBLE", "-1.234567891234", "1.25"),
    (
        "bit",
        "BIT(64)",
        "b'1010'",
        "b'1111111111111111111111111111111111111111111111111111111111111111'",
    ),
    ("date", "DATE", "'1000-01-01'", "'2024-02-29'"),
    (
        "time",
        "TIME(6)",
        "'-838:59:58.000123'",
        "'00:00:00.120000'",
    ),
    (
        "datetime",
        "DATETIME(6)",
        "'1000-01-01 00:00:00'",
        "'2024-02-29 12:34:56.123456'",
    ),
    (
        "timestamp",
        "TIMESTAMP(6)",
        "'2000-01-01 00:00:00'",
        "'2024-02-29 12:34:56.123456'",
    ),
    ("year", "YEAR", "1901", "2155"),
    ("char", "CHAR(10)", "'🐾'", "'short    '"),
    ("varchar", "VARCHAR(10)", "'🐾'", "'short    '"),
    ("binary", "BINARY(6)", "X'00FF'", "X'5C'"),
    ("varbinary", "VARBINARY(6)", "X'00FF'", "X'5C'"),
    ("tinyblob", "TINYBLOB", "X'00FF'", "X''"),
    ("blob", "BLOB", "X'00FF0D0A'", "X''"),
    ("mediumblob", "MEDIUMBLOB", "X'00FF0D0A'", "X''"),
    ("longblob", "LONGBLOB", "X'00FF0D0A'", "X''"),
    ("tinytext", "TINYTEXT", "'🐾'", "''"),
    ("text", "TEXT", "'🐾'", "''"),
    ("mediumtext", "MEDIUMTEXT", "'🐾'", "''"),
    ("longtext", "LONGTEXT", "'🐾'", "''"),
    ("enum", "ENUM('red','blue')", "'red'", "'blue'"),
    ("set", "SET('red','blue')", "'red,blue'", "''"),
    ("json", "JSON", r#"'{"b":true,"a":null}'"#, "'[1,2]'"),
    (
        "geometry",
        "GEOMETRY",
        "ST_GeomFromText('POINT(1 2)')",
        "ST_GeomFromText('LINESTRING(1 2,3 4)')",
    ),
];

const MATRIX: &str = r#"
function matrix_connect($native) {
    $db = mysqli_init();
    $db->options(MYSQLI_OPT_INT_AND_FLOAT_NATIVE, $native);
    $db->real_connect('127.0.0.1', 'phera', 'phera', 'mysqli_ext', 33069);
    $db->set_charset('utf8mb4');
    $db->query("SET time_zone = '+00:00'");
    return $db;
}
function matrix_query($db, $sql, $protocol, $procedural) {
    return match ($protocol) {
        'text' => $procedural ? mysqli_query($db, $sql) : $db->query($sql),
        'stream' => $procedural
            ? mysqli_query($db, $sql, MYSQLI_USE_RESULT)
            : $db->query($sql, MYSQLI_USE_RESULT),
        'prepared' => $procedural
            ? mysqli_execute_query($db, $sql) : $db->execute_query($sql),
    };
}
function matrix_read($r, $mode, $procedural) {
    return match ($mode) {
        'assoc' => $procedural ? mysqli_fetch_assoc($r) : $r->fetch_assoc(),
        'row' => $procedural ? mysqli_fetch_row($r) : $r->fetch_row(),
        'column' => $procedural ? mysqli_fetch_column($r) : $r->fetch_column(),
        'object' => $procedural ? mysqli_fetch_object($r) : $r->fetch_object(),
        'both' => $procedural ? mysqli_fetch_array($r, MYSQLI_BOTH) : $r->fetch_array(MYSQLI_BOTH),
        'array-num' => $procedural ? mysqli_fetch_array($r, MYSQLI_NUM) : $r->fetch_array(MYSQLI_NUM),
        'array-assoc' => $procedural
            ? mysqli_fetch_array($r, MYSQLI_ASSOC) : $r->fetch_array(MYSQLI_ASSOC),
        'all-num' => $procedural ? mysqli_fetch_all($r, MYSQLI_NUM) : $r->fetch_all(MYSQLI_NUM),
        'all-assoc' => $procedural ? mysqli_fetch_all($r, MYSQLI_ASSOC) : $r->fetch_all(MYSQLI_ASSOC),
        'all-both' => $procedural ? mysqli_fetch_all($r, MYSQLI_BOTH) : $r->fetch_all(MYSQLI_BOTH),
    };
}
function matrix_run($db, $sql, $name, $native) {
    foreach (['text', 'stream', 'prepared'] as $protocol) {
        foreach ([false, true] as $procedural) {
            $api = $procedural ? 'procedural' : 'object';
            foreach (['assoc', 'row', 'column', 'object', 'both',
                'array-num', 'array-assoc', 'all-num', 'all-assoc', 'all-both'] as $mode) {
                run("$name-$native-$protocol-$api-$mode", function () use ($db, $sql, $protocol, $procedural, $mode) {
                    $r = matrix_query($db, $sql, $protocol, $procedural);
                    try {
                        if (str_starts_with($mode, 'all-')) return matrix_read($r, $mode, $procedural);
                        return [matrix_read($r, $mode, $procedural),
                            matrix_read($r, $mode, $procedural),
                            matrix_read($r, $mode, $procedural),
                            matrix_read($r, $mode, $procedural)];
                    } finally {
                        $r->free();
                    }
                });
            }
        }
    }
}
function stream_cursors($db, $throughIterator) {
    $r = $db->query('SELECT 1 AS n UNION ALL SELECT 2', MYSQLI_USE_RESULT);
    $before = [$r->num_rows, $r->data_seek(0)];
    $rows = [];
    $cursor = $throughIterator ? $r->getIterator() : $r;
    foreach ($cursor as $k => $row) $rows[$k] = $row;
    return [$before, $rows, $r->num_rows, $r->fetch_row(), $db->query('SELECT 3')->fetch_row()];
}
function metadata_calls($db, $api) {
    $r = matrix_query($db, "SELECT 1 AS n, '🐾' AS s, NULL AS z", 'text', $api);
    if ($api) {
        $field = mysqli_fetch_field($r);
        $fields = mysqli_fetch_fields($r);
        $direct = mysqli_fetch_field_direct($r, 1);
        $seek = mysqli_field_seek($r, 2);
        $tell = mysqli_field_tell($r);
        $before = mysqli_fetch_lengths($r);
        $row = mysqli_fetch_row($r);
        $after = mysqli_fetch_lengths($r);
        $counts = [mysqli_num_fields($r), mysqli_num_rows($r)];
        $seekRow = mysqli_data_seek($r, 0);
        mysqli_free_result($r);
    } else {
        $field = $r->fetch_field();
        $fields = $r->fetch_fields();
        $direct = $r->fetch_field_direct(1);
        $seek = $r->field_seek(2);
        $tell = $r->current_field;
        $before = $r->lengths;
        $row = $r->fetch_row();
        $after = $r->lengths;
        $counts = [$r->field_count, $r->num_rows];
        $seekRow = $r->data_seek(0);
        $r->close();
    }
    return [$field, $fields, $direct, $seek, $tell, $before, $row, $after, $counts, $seekRow];
}
function bound_statement($db, $api) {
    $q = 'SELECT ? AS i, ? AS d, ? AS s, ? AS b';
    $i = -42;
    $d = 1.25;
    $s = '🐾';
    $blob = '';
    $long = "\x00\xFF";
    if ($api) {
        $st = mysqli_stmt_init($db);
        $prepared = mysqli_stmt_prepare($st, $q);
        $bound = mysqli_stmt_bind_param($st, 'idsb', $i, $d, $s, $blob);
        $sent = mysqli_stmt_send_long_data($st, 3, $long);
        $executed = mysqli_stmt_execute($st);
        $stored = mysqli_stmt_store_result($st);
        $result = mysqli_stmt_bind_result($st, $iOut, $dOut, $sOut, $bOut);
        $row = mysqli_stmt_fetch($st);
        $data = [$iOut, $dOut, $sOut, $bOut, mysqli_stmt_fetch($st)];
        $metadata = mysqli_stmt_result_metadata($st)->field_count;
        $counts = [mysqli_stmt_field_count($st), mysqli_stmt_param_count($st),
            mysqli_stmt_num_rows($st), mysqli_stmt_affected_rows($st)];
        mysqli_stmt_free_result($st);
        $reset = mysqli_stmt_reset($st);
        $close = mysqli_stmt_close($st);
    } else {
        $st = $db->stmt_init();
        $prepared = $st->prepare($q);
        $bound = $st->bind_param('idsb', $i, $d, $s, $blob);
        $sent = $st->send_long_data(3, $long);
        $executed = $st->execute();
        $stored = $st->store_result();
        $result = $st->bind_result($iOut, $dOut, $sOut, $bOut);
        $row = $st->fetch();
        $data = [$iOut, $dOut, $sOut, $bOut, $st->fetch()];
        $metadata = $st->result_metadata()->field_count;
        $counts = [$st->field_count, $st->param_count, $st->num_rows, $st->affected_rows];
        $st->free_result();
        $reset = $st->reset();
        $close = $st->close();
    }
    return [$prepared, $bound, $sent, $executed, $stored, $result, $row, $data,
        $metadata, $counts, $reset, $close];
}
function multi_calls($db, $api) {
    $sql = 'SELECT 1 AS n; SELECT 2 AS n; SELECT 3 AS n';
    $started = $api ? mysqli_multi_query($db, $sql) : $db->multi_query($sql);
    $out = [$started];
    do {
        $r = $api ? mysqli_store_result($db) : $db->store_result();
        $out[] = $r->fetch_row();
        $r->free_result();
        $out[] = $more = $api ? mysqli_more_results($db) : $db->more_results();
    } while ($more && ($api ? mysqli_next_result($db) : $db->next_result()));
    return $out;
}
function transaction_calls($db, $api) {
    $db->query('CREATE TEMPORARY TABLE matrix_transaction (n INT PRIMARY KEY)');
    if ($api) {
        $out = [mysqli_autocommit($db, false), mysqli_begin_transaction($db)];
        mysqli_query($db, 'INSERT INTO matrix_transaction VALUES (1)');
        $out[] = mysqli_savepoint($db, 's1');
        mysqli_query($db, 'INSERT INTO matrix_transaction VALUES (2)');
        mysqli_query($db, 'ROLLBACK TO SAVEPOINT s1');
        $out[] = mysqli_release_savepoint($db, 's1');
        $out[] = mysqli_commit($db);
        $out[] = mysqli_rollback($db);
        $out[] = mysqli_autocommit($db, true);
    } else {
        $out = [$db->autocommit(false), $db->begin_transaction()];
        $db->query('INSERT INTO matrix_transaction VALUES (1)');
        $out[] = $db->savepoint('s1');
        $db->query('INSERT INTO matrix_transaction VALUES (2)');
        $db->query('ROLLBACK TO SAVEPOINT s1');
        $out[] = $db->release_savepoint('s1');
        $out[] = $db->commit();
        $out[] = $db->rollback();
        $out[] = $db->autocommit(true);
    }
    $out[] = $db->query('SELECT n FROM matrix_transaction')->fetch_all();
    $db->query('DROP TEMPORARY TABLE matrix_transaction');
    return $out;
}
"#;

#[test]
fn mysqli_php_matrix() {
    let Some(comparison) = Comparison::from_environment("mysqli-matrix") else {
        return;
    };
    let fields = COLUMNS
        .iter()
        .map(|(name, kind, _, _)| format!("`{name}` {kind} NULL"))
        .collect::<Vec<_>>()
        .join(", ");
    let row = |first: bool| {
        COLUMNS
            .iter()
            .map(|(_, _, left, right)| if first { *left } else { *right })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let null = vec!["NULL"; COLUMNS.len()].join(", ");
    let mut script = format!("<?php\n{}\n{MATRIX}\n", oracle::PHP_PREAMBLE);
    script.push_str(&format!(
        "foreach ([0, 1] as $native) {{
            $db = matrix_connect($native);
            $db->query(\"CREATE TEMPORARY TABLE matrix_types (id INT PRIMARY KEY, {fields}) DEFAULT CHARSET utf8mb4\");
            $db->query(\"INSERT INTO matrix_types VALUES (1, {}), (2, {}), (3, {null})\");\n",
        row(true).replace('"', "\\\""),
        row(false).replace('"', "\\\""),
    ));
    for (name, _, _, _) in COLUMNS {
        script.push_str(&format!(
            "matrix_run($db, 'SELECT `{name}` AS v FROM matrix_types ORDER BY id', '{name}', $native);\n"
        ));
    }
    script.push_str(
        r#"
        $db->close();
    }
    $db = matrix_connect(false);
    foreach ([false, true] as $api) {
        run('transaction-' . (int)$api, fn() => transaction_calls($db, $api));
        run('stream-foreach-' . (int)$api, fn() => stream_cursors($db, $api));
        run('fields-' . (int)$api, fn() => metadata_calls($db, $api));
        run('statement-bind-' . (int)$api, fn() => bound_statement($db, $api));
        run('multi-' . (int)$api, fn() => multi_calls($db, $api));
    }
    run('stream-status', function () use ($db) {
        $db->real_query('SELECT 7 AS n UNION ALL SELECT 8');
        $result = $db->use_result();
        $before = [$result->num_rows, $result->fetch_column(), $result->num_rows];
        $failure = null;
        try { $db->query('SELECT 9'); }
        catch (mysqli_sql_exception $e) { $failure = [$e->getCode(), $e->getSqlState(), $e->getMessage()]; }
        $result->free();
        return [$before, $failure, $db->query('SELECT 9')->fetch_column()];
    });
    foreach ([0, 1, 2, 3, 4, 5, 6, 7, 255] as $report) {
        foreach (['SELECT FROM', 'SELECT missing_column FROM (SELECT 1) AS t', 'SELECT * FROM no_matrix_table'] as $i => $sql) {
            run("report-$report-$i", function () use ($db, $sql, $report) {
                mysqli_report($report);
                try {
                    $result = $db->query($sql);
                    return [$result, $db->errno, $db->error, $db->sqlstate, $db->error_list];
                } catch (mysqli_sql_exception $e) {
                    return [$e->getCode(), $e->getMessage(), $e->getSqlState()];
                } finally {
                    mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
                }
            });
        }
    }
"#,
    );
    comparison.check_matrix(&script, COLUMNS.len() * 120 + 38);
}
