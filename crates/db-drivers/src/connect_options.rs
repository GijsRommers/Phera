//! Settings such as the timeout passed to mysqli_options().

#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct ConnectOptions {
    pub timeout: Option<i64>,
    pub postgres_native: bool,
    pub mysql: MysqlOptions,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MysqlOptions {
    pub autocommit: bool,
    pub init: Option<String>,
    pub compress: bool,
    pub found_rows: bool,
    pub ignore_space: bool,
    pub multiple: bool,
    pub ssl: Option<MysqlTls>,
}

impl Default for MysqlOptions {
    fn default() -> Self {
        Self {
            autocommit: true,
            init: None,
            compress: false,
            found_rows: false,
            ignore_space: false,
            multiple: true,
            ssl: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MysqlTls {
    pub ca: Option<String>,
    pub capath: Option<String>,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub verify: bool,
}
