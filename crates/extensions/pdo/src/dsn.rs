//! DSNs select a driver: `new PDO('pgsql:host=127.0.0.1;dbname=app', 'user', 'pass')`.

use std::collections::BTreeMap;

use phera_extension_api::NativeError;

use crate::connect_options::ConnectOptions;
use crate::drivers::connection_error;
use crate::errors::DbError;

mod postgres;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DriverName {
    Postgres,
    Mysql,
    Sqlite,
}

impl DriverName {
    pub fn name(self) -> &'static str {
        match self {
            Self::Postgres => "pgsql",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }
}

/// No `Debug`: the fields may hold a `password=`.
#[derive(Clone)]
pub struct Dsn {
    pub driver: DriverName,
    pub fields: BTreeMap<String, String>,
    pub options: ConnectOptions,
}

impl Dsn {
    #[cfg(test)]
    pub fn parse(input: &str) -> Result<Self, NativeError> {
        Self::parse_named(input, "PDO::__construct")
    }

    pub fn parse_named(input: &str, method: &str) -> Result<Self, NativeError> {
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
            fields = postgres::fields(rest).map_err(|detail| {
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

    pub fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.fields.get(key).map_or(default, String::as_str)
    }

    pub fn default_port(&self) -> u16 {
        match self.driver {
            DriverName::Postgres => 5432,
            DriverName::Mysql => 3306,
            // A file has no port; nothing asks for it.
            DriverName::Sqlite => 0,
        }
    }

    /// The port, read the way the driver's C library reads it, so that a
    /// DSN connects, or fails, as it does in PHP.
    pub fn port(&self) -> Result<u16, DbError> {
        let Some(text) = self.fields.get("port") else {
            return Ok(self.default_port());
        };
        match self.driver {
            DriverName::Postgres => libpq_port(text),
            DriverName::Sqlite => Ok(0),
            DriverName::Mysql => self.mysql_port(text),
        }
    }

    fn mysql_port(&self, text: &str) -> Result<u16, DbError> {
        // mysqlnd passes atoi() as unsigned, then PHP's TCP transport checks the range.
        let port = atoi(text) as u32;
        if port == 0 {
            return Ok(self.default_port());
        }
        u16::try_from(port).map_err(|_| {
            let host = self.get("host", "localhost");
            let address = if host.parse::<std::net::Ipv6Addr>().is_ok() {
                format!("[{host}]:{port}")
            } else {
                format!("{host}:{port}")
            };
            DbError {
                state: "HY000".into(),
                code: Some(2002),
                detail: format!("Failed to parse port of address \"{address}\""),
            }
        })
    }
}

/// libpq: an `int`, perhaps with white space around it, from 1 to 65535.
fn libpq_port(text: &str) -> Result<u16, DbError> {
    let number: i32 = text.trim_matches(c_space).parse().map_err(|_| {
        connection_error(format!(
            "invalid integer value \"{text}\" for connection option \"port\""
        ))
    })?;
    u16::try_from(number)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| connection_error(format!("invalid port number: \"{text}\"")))
}

/// C's `atoi()`: white space, a sign, then digits up to the first other
/// character; 0 when there are none.
fn atoi(text: &str) -> i64 {
    let text = text.trim_start_matches(c_space);
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let number = unsigned
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0_i64, |number, digit| {
            number
                .saturating_mul(10)
                .saturating_add(i64::from(digit - b'0'))
        });
    if negative { -number } else { number }
}

/// C's `isspace()`.
fn c_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

#[cfg(test)]
mod tests {
    use super::{DriverName, Dsn};

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
