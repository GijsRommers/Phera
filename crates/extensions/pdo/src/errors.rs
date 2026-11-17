//! `$pdo->errorInfo()` returns `[SQLSTATE, driver_code, driver_message]`.

use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value};

#[derive(Clone, Copy, Debug, Default)]
pub enum ErrorMode {
    Silent,
    Warning,
    #[default]
    Exception,
}

#[derive(Clone, Debug)]
pub struct DbError {
    pub state: String,
    pub code: Option<i64>,
    pub detail: String,
}

impl DbError {
    pub fn parameter(detail: &str) -> Self {
        Self {
            state: "HY093".into(),
            code: None,
            detail: detail.into(),
        }
    }

    pub fn message(&self) -> String {
        let description = match self.state.as_str() {
            "00000" => "No error",
            "HY093" => "Invalid parameter number",
            "42P01" => "Undefined table",
            "42P02" => "Undefined parameter",
            "42P18" => "Indeterminate datatype",
            "42703" => "Undefined column",
            "42601" => "Syntax error",
            "42000" => "Syntax error or access violation",
            "23505" => "Unique violation",
            "23000" => "Integrity constraint violation",
            "23502" => "Not null violation",
            "23503" => "Foreign key violation",
            "22P02" => "Invalid text representation",
            "22012" => "Division by zero",
            "08006" => "Connection failure",
            "08P01" => "<<Unknown error>>",
            "42S02" => "Base table or view not found",
            "42S22" => "Column not found",
            "IM001" => "Driver does not support this function",
            _ => "General error",
        };
        let mut message = format!("SQLSTATE[{}]: {description}", self.state);
        if let Some(code) = self.code {
            message.push_str(&format!(": {code}"));
        }
        if !self.detail.is_empty() {
            let separator = if self.code.is_some() { " " } else { ": " };
            message.push_str(separator);
            message.push_str(&self.detail);
        }
        message
    }

    pub fn report(&self, context: &mut dyn Context, mode: ErrorMode, method: &str) -> NativeResult {
        match mode {
            ErrorMode::Silent => Ok(Value::Bool(false)),
            ErrorMode::Warning => {
                context.warning(format!("{method}(): {}", self.message()));
                Ok(Value::Bool(false))
            }
            ErrorMode::Exception => Err(exception(self.message(), Some(self), 0)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum ErrorState {
    #[default]
    Unused,
    UnusedDriver(DbError),
    Success,
    Failed(DbError),
}

impl ErrorState {
    pub fn code(&self) -> Value {
        match self {
            Self::Unused | Self::UnusedDriver(_) => Value::Null,
            Self::Success => Value::from("00000"),
            Self::Failed(error) => Value::from(error.state.clone()),
        }
    }

    pub fn info(&self) -> NativeResult {
        let mut info = PhpArray::new();
        info.push(match self {
            Self::Unused | Self::UnusedDriver(_) => Value::from(""),
            _ => self.code(),
        })?;
        match self {
            Self::Failed(error) | Self::UnusedDriver(error) => {
                info.push(error.code.map_or(Value::Null, Value::Int))?;
                info.push(if error.detail.is_empty() {
                    Value::Null
                } else {
                    Value::from(error.detail.clone())
                })?;
            }
            _ => {
                info.push(Value::Null)?;
                info.push(Value::Null)?;
            }
        }
        Ok(Value::from(info))
    }
}

pub fn unsupported(feature: &str) -> NativeError {
    exception(format!("{feature} is not supported yet"), None, 0)
}

/// Every PDOException is built here.
///
/// ```php
/// try { $pdo->query('SELECT * FROM missing'); }
/// catch (PDOException $e) {
///     $e->getCode();     // "42P01": the SQLSTATE, a string
///     $e->errorInfo;     // ["42P01", 7, "ERROR:  relation ... does not exist"]
/// }
/// ```
///
/// With a driver error, the exception carries the SQLSTATE as its code and the
/// three-element `errorInfo` array. A failed connection is the one case where
/// PHP reports the driver's own number as the code, so a non-zero `code` wins.
/// Without a driver error (a misuse caught by PDO itself) there is only the
/// message and the numeric code.
pub fn exception(message: impl Into<String>, error: Option<&DbError>, code: i64) -> NativeError {
    let thrown = NativeError::throw("PDOException", message);
    let Some(error) = error else {
        return thrown.with_code(code);
    };
    let thrown = if code == 0 {
        thrown.with_code(error.state.as_str())
    } else {
        thrown.with_code(code)
    };
    if error.state == "IM001" && error.detail == "driver does not support that attribute" {
        let mut information = PhpArray::new();
        information.insert(phera_extension_api::ArrayKey::Int(0), Value::from("IM001"));
        information.insert(phera_extension_api::ArrayKey::Int(1), Value::Int(0));
        return thrown.with_property("errorInfo", Value::from(information));
    }
    match ErrorState::Failed(error.clone()).info() {
        Ok(information) => thrown.with_property("errorInfo", information),
        Err(_) => thrown,
    }
}

#[cfg(test)]
mod tests {
    use super::DbError;

    #[test]
    fn formats_driver_and_parameter_errors() {
        let error = DbError {
            state: "42P01".into(),
            code: Some(7),
            detail: "ERROR:  relation \"missing\" does not exist".into(),
        };
        assert_eq!(
            error.message(),
            "SQLSTATE[42P01]: Undefined table: 7 ERROR:  relation \"missing\" does not exist"
        );
        assert_eq!(
            DbError::parameter("mixed named and positional parameters").message(),
            "SQLSTATE[HY093]: Invalid parameter number: mixed named and positional parameters"
        );
    }
}
