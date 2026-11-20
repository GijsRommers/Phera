//! DSNs select a driver: `new PDO('pgsql:host=127.0.0.1;dbname=app', 'user', 'pass')`.

use std::collections::BTreeMap;

use phera_extension_api::NativeError;

use crate::drivers::connection_error;
use phera_db_drivers::connect_options::ConnectOptions;
use phera_db_drivers::dsn::postgres;
pub use phera_db_drivers::dsn::{DriverName, Dsn};

pub trait ParseDsn {
    #[cfg(test)]
    fn parse(input: &str) -> Result<Dsn, NativeError> {
        Self::parse_named(input, "PDO::__construct")
    }

    fn parse_named(input: &str, method: &str) -> Result<Dsn, NativeError>;
}

impl ParseDsn for Dsn {
    fn parse_named(input: &str, method: &str) -> Result<Dsn, NativeError> {
        let (name, rest) = input.split_once(':').ok_or_else(|| {
            crate::errors::exception(
                format!("{method}(): Argument #1 ($dsn) must be a valid data source name"),
                None,
                0,
            )
        })?;
        let driver = match name {
            "pgsql" => DriverName::Postgres,
            "mysql" => DriverName::Mysql,
            "sqlite" => DriverName::Sqlite,
            _ => return Err(crate::errors::exception("could not find driver", None, 0)),
        };
        let mut fields = BTreeMap::new();
        // `sqlite:/path/to/file`, `sqlite::memory:`: everything after the prefix
        // is the file name, semicolons included.
        if driver == DriverName::Sqlite {
            fields.insert("filename".into(), rest.into());
            return Ok(Self {
                driver,
                fields,
                options: ConnectOptions::default(),
            });
        }
        if driver == DriverName::Postgres {
            fields = postgres::fields(rest, true).map_err(|detail| {
                let error = connection_error(detail);
                crate::errors::exception(
                    format!("SQLSTATE[08006] [7] {}", error.detail),
                    Some(&error),
                    7,
                )
            })?;
        } else {
            for field in rest.split(';').filter(|field| !field.trim().is_empty()) {
                if let Some((key, value)) = field.split_once('=') {
                    fields.insert(key.trim().to_owned(), value.trim().to_owned());
                }
            }
        }
        Ok(Self {
            driver,
            fields,
            options: ConnectOptions::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DriverName, Dsn, ParseDsn};

    #[test]
    fn parses_driver_and_values_containing_equals() {
        let dsn = Dsn::parse("pgsql:host=localhost;port=54329;password=a=b").unwrap();
        assert_eq!(dsn.driver, DriverName::Postgres);
        assert_eq!(dsn.port().unwrap(), 54329);
        assert_eq!(dsn.get("password", ""), "a=b");
    }

    #[test]
    fn preserves_tls_fields_and_rejects_unknown_drivers() {
        let dsn = Dsn::parse("pgsql:sslmode=require;sslrootcert=ca.pem").unwrap();
        assert_eq!(dsn.get("sslmode", ""), "require");
        assert_eq!(dsn.get("sslrootcert", ""), "ca.pem");
        assert!(Dsn::parse("unknown:host=localhost").is_err());
        assert_eq!(Dsn::parse("sqlite:foo").unwrap().get("filename", ""), "foo");
        assert!(Dsn::parse("pgsql:sslmode=disable").is_ok());
    }

    #[test]
    fn reads_ports_as_the_c_libraries_do() {
        let port = |dsn: &str| {
            Dsn::parse(dsn)
                .unwrap()
                .port()
                .map_err(|error| error.detail)
        };
        assert_eq!(port("pgsql:port=+54329"), Ok(54329));
        assert_eq!(
            port("pgsql:port=1e3"),
            Err("invalid integer value \"1e3\" for connection option \"port\"".into())
        );
        assert_eq!(
            port("pgsql:port=65536"),
            Err("invalid port number: \"65536\"".into())
        );
        assert_eq!(port("mysql:port=33069abc"), Ok(33069));
        assert_eq!(
            port("mysql:host=127.0.0.1;port=98605"),
            Err("Failed to parse port of address \"127.0.0.1:98605\"".into())
        );
        assert_eq!(port("mysql:port=x"), Ok(3306));
    }
}
