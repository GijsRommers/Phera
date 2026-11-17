//! DSNs select a driver: `new PDO('pgsql:host=127.0.0.1;dbname=app', 'user', 'pass')`.

use std::collections::BTreeMap;

use phera_extension_api::NativeError;

use crate::drivers::connection_error;
use crate::errors::DbError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DriverName {
    Postgres,
    Mysql,
}

impl DriverName {
    pub fn name(self) -> &'static str {
        match self {
            Self::Postgres => "pgsql",
            Self::Mysql => "mysql",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Dsn {
    pub driver: DriverName,
    pub fields: BTreeMap<String, String>,
}

impl Dsn {
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
            "sqlite" => return Err(crate::errors::unsupported("SQLite")),
            _ => return Err(crate::errors::exception("could not find driver", None, 0)),
        };
        let mut fields = BTreeMap::new();
        for field in rest.split(';').filter(|field| !field.trim().is_empty()) {
            if let Some((key, value)) = field.split_once('=') {
                fields.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }
        for (key, value) in &fields {
            if key.starts_with("ssl") && !(key == "sslmode" && value == "disable") {
                return Err(crate::errors::exception(
                    "TLS is not supported yet",
                    None,
                    0,
                ));
            }
        }
        Ok(Self { driver, fields })
    }

    pub fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.fields.get(key).map_or(default, String::as_str)
    }

    pub fn default_port(&self) -> u16 {
        match self.driver {
            DriverName::Postgres => 5432,
            DriverName::Mysql => 3306,
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
            // mysqlnd takes `atoi()` as an unsigned int, 0 meaning the
            // default, and the socket keeps its low 16 bits: 98605 is 33069.
            DriverName::Mysql => Ok(match atoi(text) as u32 {
                0 => self.default_port(),
                port => port as u16,
            }),
        }
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
    fn rejects_tls_and_unknown_drivers() {
        assert!(Dsn::parse("pgsql:sslmode=require").is_err());
        assert!(Dsn::parse("mysql:ssl_ca=ca.pem").is_err());
        assert!(Dsn::parse("sqlite:foo").is_err());
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
        assert_eq!(port("mysql:port=98605"), Ok(33069));
        assert_eq!(port("mysql:port=x"), Ok(3306));
    }
}
