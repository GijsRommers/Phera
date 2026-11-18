//! Native drivers also accept numeric bindings for named SQL placeholders.

use std::collections::HashMap;

use crate::dsn::DriverName;
use crate::errors::DbError;
use crate::parameters::Binding;
use crate::placeholders::{Parameter, Sql};

pub fn ordered<'a>(
    sql: &Sql,
    bindings: &'a HashMap<String, Binding>,
    driver: DriverName,
) -> Result<Vec<&'a Binding>, DbError> {
    // With no PDO tokens, PHP accepts numeric bindings but sends SQL NULLs.
    // Native $n tokens are left for PostgreSQL and do not create a PDO parameter map.
    if driver == DriverName::Postgres && sql.parameters.is_empty() {
        for key in bindings.keys() {
            if key.parse::<usize>().is_err() {
                return Err(DbError::parameter(&format!(":{key}")));
            }
        }
        return Ok(bindings.values().collect());
    }
    let named = sql
        .parameters
        .iter()
        .any(|parameter| matches!(parameter, Parameter::Name(_)));
    for key in bindings.keys() {
        if !valid_key(sql, key) {
            let detail = match driver {
                DriverName::Postgres if key.parse::<usize>().is_err() => format!(":{key}"),
                DriverName::Mysql if !named => String::new(),
                _ => "parameter was not defined".into(),
            };
            return Err(DbError::parameter(&detail));
        }
    }
    let mut ordered = Vec::new();
    for (index, parameter) in sql.parameters.iter().enumerate() {
        let binding = match parameter {
            Parameter::Name(name) => bindings.get(name),
            Parameter::Position(_) => None,
        }
        .or_else(|| bindings.get(&index.to_string()));
        let Some(binding) = binding else {
            if driver == DriverName::Postgres {
                continue;
            }
            return Err(DbError::parameter(match driver {
                DriverName::Mysql | DriverName::Sqlite => "",
                DriverName::Postgres => "number of bound variables does not match number of tokens",
            }));
        };
        ordered.push(binding);
    }
    if driver == DriverName::Mysql && bindings.len() < sql.parameters.len() {
        return Err(DbError::parameter(""));
    }
    Ok(ordered)
}

pub fn valid_key(sql: &Sql, key: &str) -> bool {
    sql.parameters.iter().enumerate().any(|(index, parameter)| {
        key == index.to_string() || matches!(parameter, Parameter::Name(name) if *name == key)
    })
}
