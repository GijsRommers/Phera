//! Connection settings such as the host and port passed to mysqli_connect().

use std::collections::BTreeMap;

use crate::connect_options::ConnectOptions;
use crate::drivers::connection_error;
use crate::errors::DbError;

pub mod postgres;

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
