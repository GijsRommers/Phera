//! `new PDO($dsn, $user, $password, [PDO::ATTR_TIMEOUT => 5])` sets a dial deadline.

use phera_extension_api::value::{to_int, to_php_string};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

use crate::constants::*;
use crate::dsn::DriverName;
use crate::errors::unsupported;

use phera_db_drivers::connect_options::{ConnectOptions, MysqlOptions, MysqlTls};

pub trait FromPhp {
    fn from_php(
        context: &mut dyn Context,
        driver: DriverName,
        options: &PhpArray,
    ) -> Result<ConnectOptions, NativeError>;
}

impl FromPhp for ConnectOptions {
    fn from_php(
        context: &mut dyn Context,
        driver: DriverName,
        options: &PhpArray,
    ) -> Result<Self, NativeError> {
        let mut number = |attribute, default| {
            options
                .get(&ArrayKey::Int(attribute))
                .map_or(Ok(default), |value| to_int(value, context.diagnostics()))
        };
        let timeout = if options.get(&ArrayKey::Int(ATTR_TIMEOUT)).is_some() {
            Some(number(ATTR_TIMEOUT, 30)?)
        } else {
            None
        };
        if driver != DriverName::Mysql {
            return Ok(Self {
                timeout,
                ..Self::default()
            });
        }
        let autocommit = number(ATTR_AUTOCOMMIT, 1)? != 0;
        let compress = number(MYSQL_COMPRESS, 0)? != 0;
        let found_rows = number(MYSQL_FOUND_ROWS, 0)? != 0;
        let ignore_space = number(MYSQL_IGNORE_SPACE, 0)? != 0;
        let local_infile = number(MYSQL_LOCAL_INFILE, 0)? != 0;
        let multiple = number(MYSQL_MULTI_STATEMENTS, 1)? != 0;
        let verify = number(MYSQL_SSL_VERIFY, 1)? != 0;
        if local_infile
            || options
                .get(&ArrayKey::Int(MYSQL_LOCAL_INFILE_DIRECTORY))
                .is_some()
        {
            return Err(crate::errors::exception(
                "MySQL LOCAL INFILE is disabled because it lets the server read client files",
                None,
                0,
            ));
        }
        if options
            .get(&ArrayKey::Int(MYSQL_SERVER_PUBLIC_KEY))
            .is_some()
        {
            return Err(unsupported("Pinning a MySQL authentication public key"));
        }
        let mut string = |attribute| {
            options
                .get(&ArrayKey::Int(attribute))
                .map(|value: &Value| {
                    to_php_string(value, context.diagnostics()).map(|text| text.to_string_lossy())
                })
                .transpose()
        };
        let init = string(MYSQL_INIT)?;
        let ca = string(MYSQL_SSL_CA)?;
        let capath = string(MYSQL_SSL_CAPATH)?;
        let cert = string(MYSQL_SSL_CERT)?;
        let key = string(MYSQL_SSL_KEY)?;
        let cipher = string(MYSQL_SSL_CIPHER)?;
        let ssl = (ca.is_some()
            || capath.is_some()
            || cert.is_some()
            || key.is_some()
            || cipher.is_some())
        .then_some(MysqlTls {
            ca,
            capath,
            cert,
            key,
            verify,
        });
        Ok(Self {
            timeout,
            mysql: MysqlOptions {
                autocommit,
                init,
                compress,
                found_rows,
                ignore_space,
                multiple,
                ssl,
            },
        })
    }
}
