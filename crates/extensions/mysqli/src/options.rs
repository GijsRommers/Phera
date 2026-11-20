//! `mysqli_options($db, MYSQLI_OPT_INT_AND_FLOAT_NATIVE, true)` changes text results.

use phera_db_drivers::connect_options::{MysqlOptions, MysqlTls};
use phera_extension_api::value::{to_bool, to_int, to_php_string};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::connection::Connection;

pub fn option(
    context: &mut dyn Context,
    state: &mut Connection,
    arguments: &[Value],
) -> NativeResult {
    let value = args::value(arguments, 1);
    match args::int(arguments, 0).unwrap_or_default() {
        0 => state.options.timeout = Some(to_int(value, context.diagnostics())?),
        3 => {
            state.options.mysql.init =
                Some(to_php_string(value, context.diagnostics())?.to_string_lossy())
        }
        7 => state.charset = Some(to_php_string(value, context.diagnostics())?.to_string_lossy()),
        8 if !to_bool(value) => {}
        8 | 43 => {
            return Err(NativeError::error(
                "MySQL LOCAL INFILE is disabled because it lets the server read client files",
            ));
        }
        21 => ssl(&mut state.options.mysql).verify = to_bool(value),
        201 => state.native_numbers = to_bool(value),
        _ => return Ok(Value::Bool(false)),
    }
    Ok(Value::Bool(true))
}

pub fn ssl_set(options: &mut MysqlOptions, arguments: &[Value]) {
    let str_at = |i| args::string(arguments, i).map(|value| value.to_string_lossy());
    let settings = ssl(options);
    settings.key = str_at(0);
    settings.cert = str_at(1);
    settings.ca = str_at(2);
    settings.capath = str_at(3);
}

pub fn flags(options: &mut MysqlOptions, flags: i64) -> Result<(), NativeError> {
    if flags & 128 != 0 {
        return Err(NativeError::error(
            "MySQL LOCAL INFILE is disabled because it lets the server read client files",
        ));
    }
    options.found_rows = flags & 2 != 0;
    options.compress = flags & 32 != 0;
    options.ignore_space = flags & 256 != 0;
    if flags & (2048 | 64 | 1073741824) != 0 {
        let settings = ssl(options);
        if flags & 64 != 0 {
            settings.verify = false;
        }
        if flags & 1073741824 != 0 {
            settings.verify = true;
        }
    }
    Ok(())
}

fn ssl(options: &mut MysqlOptions) -> &mut MysqlTls {
    options.ssl.get_or_insert(MysqlTls {
        ca: None,
        capath: None,
        cert: None,
        key: None,
        verify: true,
    })
}
