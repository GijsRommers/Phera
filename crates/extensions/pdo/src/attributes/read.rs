//! `$pdo->getAttribute(PDO::ATTR_SERVER_INFO)` may query the database asynchronously.

use std::rc::Rc;

use phera_extension_api::{Context, LocalFuture, NativeResult, Object, Value, args};

use super::ColumnCase;
use crate::connection::session;
use crate::constants::*;
use crate::dsn::DriverName;
use crate::errors::{DbError, ErrorMode};

pub fn get<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let connection = session(this.as_ref())?;
        connection.success();
        let attribute = args::int(&arguments, 0).unwrap_or_default();
        if connection.name != DriverName::Sqlite
            && matches!(attribute, ATTR_CONNECTION_STATUS | ATTR_SERVER_INFO)
        {
            let result = connection.driver.server_attribute(attribute).await;
            return match result {
                Ok(text) => Ok(Value::from(text)),
                Err(error) => connection.fail(context, error, "PDO::getAttribute"),
            };
        }
        let attrs = connection.attributes.borrow();
        let value = match (connection.name, attribute) {
            (_, ATTR_ERRMODE) => Value::Int(match attrs.errors {
                ErrorMode::Silent => ERRMODE_SILENT,
                ErrorMode::Warning => ERRMODE_WARNING,
                ErrorMode::Exception => ERRMODE_EXCEPTION,
            }),
            (_, ATTR_SERVER_VERSION) | (DriverName::Sqlite, ATTR_CLIENT_VERSION) => {
                Value::from(connection.version.clone())
            }
            (DriverName::Mysql, ATTR_CLIENT_VERSION) => Value::from("mysql_async 0.37.1"),
            (DriverName::Postgres, ATTR_CLIENT_VERSION) => Value::from("phera-postgres 3.0"),
            (_, ATTR_CASE) => Value::Int(match attrs.case {
                ColumnCase::Natural => CASE_NATURAL,
                ColumnCase::Upper => CASE_UPPER,
                ColumnCase::Lower => CASE_LOWER,
            }),
            (_, ATTR_PERSISTENT) => Value::Bool(attrs.persistent),
            (_, ATTR_DRIVER_NAME) => Value::from(connection.name.name()),
            (_, ATTR_STRINGIFY_FETCHES) => Value::Bool(attrs.stringify),
            (_, ATTR_ORACLE_NULLS) => Value::Int(attrs.oracle_nulls.number()),
            (_, ATTR_STATEMENT_CLASS) => attrs.statement_class.as_attribute()?,
            (_, ATTR_DEFAULT_FETCH_MODE) => Value::Int(attrs.fetch.number() | attrs.fetch_flags),
            (DriverName::Sqlite, SQLITE_ATTR_TRANSACTION_MODE) => {
                Value::Int(attrs.transaction_mode)
            }
            (DriverName::Mysql | DriverName::Postgres, ATTR_EMULATE_PREPARES) => {
                Value::Bool(attrs.emulate)
            }
            (DriverName::Mysql, ATTR_AUTOCOMMIT) => Value::Bool(attrs.autocommit),
            (DriverName::Mysql, ATTR_DEFAULT_STR_PARAM) => Value::Int(if attrs.national_strings {
                STR_NATL
            } else {
                STR_CHAR
            }),
            (DriverName::Mysql, ATTR_FETCH_TABLE_NAMES) => Value::Bool(attrs.mysql_table_names),
            (DriverName::Mysql, MYSQL_BUFFERED) => Value::Bool(attrs.mysql_buffered),
            (DriverName::Mysql, MYSQL_LOCAL_INFILE) => Value::Bool(false),
            (DriverName::Mysql, MYSQL_LOCAL_INFILE_DIRECTORY) => Value::Null,
            (DriverName::Postgres, PGSQL_DISABLE_PREPARES) => {
                Value::Bool(attrs.pgsql_disable_prepares)
            }
            _ => {
                drop(attrs);
                let error = DbError {
                    state: "IM001".into(),
                    code: None,
                    detail: "driver does not support that attribute".into(),
                };
                return connection.fail_get_attribute(context, error);
            }
        };
        Ok(value)
    })
}
