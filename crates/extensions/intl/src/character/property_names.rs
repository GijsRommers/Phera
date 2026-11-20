//! ICU's public property IDs index Unicode property aliases in IntlChar.
//!
//! ```php
//! IntlChar::getPropertyName(IntlChar::PROPERTY_GENERAL_CATEGORY); // "General_Category"
//! IntlChar::getPropertyValueName(IntlChar::PROPERTY_GENERAL_CATEGORY, 1); // "Uppercase_Letter"
//! ```

use phera_extension_api::{Context, NativeResult, Value, args};

use crate::error;

pub(super) fn name(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    lookup(context, values, false, "IntlChar::getPropertyName")
}

pub(super) fn value_name(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    lookup(context, values, true, "IntlChar::getPropertyValueName")
}

fn lookup(
    context: &mut dyn Context,
    values: &[Value],
    value_name: bool,
    site: &str,
) -> NativeResult {
    error::clear(context);
    let property = args::int(values, 0).unwrap_or_default() as i32;
    let key = if value_name {
        let value = args::int(values, 1).unwrap_or_default() as i32;
        format!("{property}:{value}")
    } else {
        property.to_string()
    };
    let choice = args::int(values, if value_name { 2 } else { 1 }).unwrap_or(1) as i32;
    let choice = choice.to_string();
    let name = include_str!("../../data/unicode-property-names.tsv")
        .lines()
        .find_map(|row| {
            let (id, rest) = row.split_once('\t')?;
            if id != key {
                return None;
            }
            let (index, alias) = rest.split_once('\t')?;
            (index == choice).then_some(alias)
        });
    if let Some(name) = name {
        Ok(Value::from(name))
    } else {
        error::store(context, site, 1, "Failed to get property name");
        Ok(Value::Bool(false))
    }
}
