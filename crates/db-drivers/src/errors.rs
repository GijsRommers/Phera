//! A server error, such as the SQLSTATE from mysqli_sql_exception::getSqlState().

#[derive(Clone, Debug)]
pub struct DbError {
    pub state: String,
    pub code: Option<i64>,
    pub detail: String,
}

impl DbError {
    /// `SQLSTATE[HY000]: General error: ...`, for failures that are not the
    /// server's: a bad setting, a feature that is missing.
    pub fn general(detail: impl Into<String>) -> Self {
        Self {
            state: "HY000".into(),
            code: None,
            detail: detail.into(),
        }
    }

    pub fn parameter(detail: &str) -> Self {
        Self {
            state: "HY093".into(),
            code: None,
            detail: detail.into(),
        }
    }
}
