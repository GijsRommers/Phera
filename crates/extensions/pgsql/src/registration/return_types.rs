//! PHP 8.5 return declarations for calls such as `pg_query($db, $sql)`.

use phera_extension_api::Registry;

pub(super) fn register(registry: &mut Registry) {
    for &(returns, functions) in RETURN_TYPES {
        for &name in functions {
            registry.describe_function(name, returns, None);
        }
    }
}

const RETURN_TYPES: &[(&str, &[&str])] = &[
    ("PgSql\\Connection|false", &["pg_connect", "pg_pconnect"]),
    (
        "PgSql\\Result|false",
        &[
            "pg_exec",
            "pg_execute",
            "pg_get_result",
            "pg_prepare",
            "pg_query",
            "pg_query_params",
        ],
    ),
    ("PgSql\\Result|string|bool", &["pg_insert"]),
    (
        "array",
        &["pg_fetch_all", "pg_fetch_all_columns", "pg_version"],
    ),
    (
        "array|false",
        &[
            "pg_convert",
            "pg_copy_to",
            "pg_fetch_array",
            "pg_fetch_assoc",
            "pg_fetch_row",
            "pg_get_notify",
            "pg_meta_data",
        ],
    ),
    ("array|string|bool", &["pg_last_notice"]),
    ("array|string|false", &["pg_select"]),
    (
        "bool",
        &[
            "pg_connection_busy",
            "pg_consume_input",
            "pg_copy_from",
            "pg_end_copy",
            "pg_free_result",
            "pg_freeresult",
            "pg_ping",
            "pg_put_line",
            "pg_result_seek",
        ],
    ),
    (
        "int",
        &[
            "pg_affected_rows",
            "pg_cmdtuples",
            "pg_connect_poll",
            "pg_connection_status",
            "pg_field_num",
            "pg_field_size",
            "pg_fieldnum",
            "pg_fieldsize",
            "pg_get_pid",
            "pg_num_fields",
            "pg_num_rows",
            "pg_numfields",
            "pg_numrows",
            "pg_set_client_encoding",
            "pg_setclientencoding",
            "pg_transaction_status",
        ],
    ),
    (
        "int|bool",
        &[
            "pg_send_execute",
            "pg_send_prepare",
            "pg_send_query",
            "pg_send_query_params",
        ],
    ),
    ("int|false", &["pg_field_is_null", "pg_field_prtlen"]),
    ("object|false", &["pg_fetch_object"]),
    (
        "string",
        &[
            "pg_client_encoding",
            "pg_clientencoding",
            "pg_dbname",
            "pg_errormessage",
            "pg_escape_bytea",
            "pg_escape_string",
            "pg_field_name",
            "pg_field_type",
            "pg_fieldname",
            "pg_fieldtype",
            "pg_host",
            "pg_last_error",
            "pg_options",
            "pg_port",
            "pg_unescape_bytea",
        ],
    ),
    ("string|bool", &["pg_delete", "pg_update"]),
    (
        "string|false",
        &[
            "pg_escape_identifier",
            "pg_escape_literal",
            "pg_parameter_status",
            "pg_result_error",
        ],
    ),
    (
        "string|false|null",
        &["pg_fetch_result", "pg_result", "pg_result_error_field"],
    ),
    ("string|int", &["pg_field_type_oid", "pg_result_status"]),
    (
        "string|int|false",
        &["pg_field_table", "pg_getlastoid", "pg_last_oid"],
    ),
    ("true", &["pg_close"]),
];
