//! DSNs select a driver: `new PDO('pgsql:host=127.0.0.1;dbname=app', 'user', 'pass')`.

use std::collections::BTreeMap;

use phera_extension_api::NativeError;

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
            "sqlite" => DriverName::Sqlite,
            _ => return Err(crate::errors::exception("could not find driver", None, 0)),
        };
        let mut fields = BTreeMap::new();
        if driver == DriverName::Sqlite {
            fields.insert("filename".into(), rest.into());
            return Ok(Self { driver, fields });
        }
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

    pub fn port(&self) -> Result<u16, NativeError> {
        let default = match self.driver {
            DriverName::Postgres => "5432",
            DriverName::Mysql => "3306",
            DriverName::Sqlite => "0",
        };
        self.get("port", default)
            .parse()
            .map_err(|_| crate::errors::exception("Invalid port in data source name", None, 0))
    }
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
        assert_eq!(Dsn::parse("sqlite:foo").unwrap().get("filename", ""), "foo");
        assert!(Dsn::parse("pgsql:sslmode=disable").is_ok());
    }
}
