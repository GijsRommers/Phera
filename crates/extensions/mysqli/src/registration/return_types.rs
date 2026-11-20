//! PHP 8.5 return declarations for calls such as `mysqli_query($db, $sql)`.

use phera_extension_api::Registry;

pub(super) fn register(registry: &mut Registry) {
    for &(returns, functions) in RETURN_TYPES {
        for &name in functions {
            registry.describe_function(name, returns, None);
        }
    }
}

const RETURN_TYPES: &[(&str, &[&str])] = &[
    ("?bool", &["mysqli_stmt_fetch"]),
    ("?object", &["mysqli_get_charset"]),
    ("?string", &["mysqli_connect_error", "mysqli_info"]),
    (
        "array",
        &[
            "mysqli_error_list",
            "mysqli_fetch_all",
            "mysqli_fetch_fields",
            "mysqli_stmt_error_list",
        ],
    ),
    ("array|false", &["mysqli_fetch_lengths"]),
    (
        "array|false|null",
        &[
            "mysqli_fetch_array",
            "mysqli_fetch_assoc",
            "mysqli_fetch_row",
        ],
    ),
    (
        "bool",
        &[
            "mysqli_autocommit",
            "mysqli_begin_transaction",
            "mysqli_change_user",
            "mysqli_commit",
            "mysqli_data_seek",
            "mysqli_kill",
            "mysqli_more_results",
            "mysqli_multi_query",
            "mysqli_next_result",
            "mysqli_options",
            "mysqli_ping",
            "mysqli_real_connect",
            "mysqli_real_query",
            "mysqli_release_savepoint",
            "mysqli_rollback",
            "mysqli_savepoint",
            "mysqli_select_db",
            "mysqli_set_charset",
            "mysqli_set_opt",
            "mysqli_stmt_bind_param",
            "mysqli_stmt_bind_result",
            "mysqli_stmt_execute",
            "mysqli_stmt_more_results",
            "mysqli_stmt_next_result",
            "mysqli_stmt_prepare",
            "mysqli_stmt_reset",
            "mysqli_stmt_send_long_data",
            "mysqli_stmt_store_result",
        ],
    ),
    (
        "int",
        &[
            "mysqli_connect_errno",
            "mysqli_errno",
            "mysqli_field_count",
            "mysqli_field_tell",
            "mysqli_get_client_version",
            "mysqli_get_proto_info",
            "mysqli_get_server_version",
            "mysqli_num_fields",
            "mysqli_stmt_errno",
            "mysqli_stmt_field_count",
            "mysqli_stmt_param_count",
            "mysqli_thread_id",
            "mysqli_warning_count",
        ],
    ),
    (
        "mysqli_result|bool",
        &["mysqli_execute_query", "mysqli_query"],
    ),
    (
        "mysqli_result|false",
        &[
            "mysqli_stmt_get_result",
            "mysqli_stmt_result_metadata",
            "mysqli_store_result",
            "mysqli_use_result",
        ],
    ),
    ("mysqli_stmt|false", &["mysqli_prepare", "mysqli_stmt_init"]),
    ("mysqli_warning|false", &["mysqli_get_warnings"]),
    ("mysqli|false", &["mysqli_connect", "mysqli_init"]),
    (
        "object|false",
        &["mysqli_fetch_field", "mysqli_fetch_field_direct"],
    ),
    ("object|false|null", &["mysqli_fetch_object"]),
    (
        "string",
        &[
            "mysqli_character_set_name",
            "mysqli_error",
            "mysqli_escape_string",
            "mysqli_get_client_info",
            "mysqli_get_host_info",
            "mysqli_get_server_info",
            "mysqli_real_escape_string",
            "mysqli_sqlstate",
            "mysqli_stmt_error",
            "mysqli_stmt_sqlstate",
        ],
    ),
    ("string|false", &["mysqli_stat"]),
    (
        "string|int",
        &[
            "mysqli_affected_rows",
            "mysqli_insert_id",
            "mysqli_num_rows",
            "mysqli_stmt_affected_rows",
            "mysqli_stmt_insert_id",
            "mysqli_stmt_num_rows",
        ],
    ),
    ("string|int|float|false|null", &["mysqli_fetch_column"]),
    (
        "true",
        &[
            "mysqli_close",
            "mysqli_field_seek",
            "mysqli_report",
            "mysqli_ssl_set",
            "mysqli_stmt_close",
        ],
    ),
    (
        "void",
        &[
            "mysqli_free_result",
            "mysqli_stmt_data_seek",
            "mysqli_stmt_free_result",
        ],
    ),
];
