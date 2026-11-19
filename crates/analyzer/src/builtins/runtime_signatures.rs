//! `class InvoiceDate extends DateTime {}` inherits factories that create InvoiceDates.
//! Some php-src signatures name the base class even though PHP uses the called class.

use crate::{index::Index, types::ValueType};

pub fn refine(index: &mut Index) {
    for class in ["datetime", "datetimeimmutable"] {
        let Some(class) = index.classes.get_mut(class) else {
            continue;
        };
        for name in [
            "createfromformat",
            "createfrominterface",
            "createfrommutable",
            "createfromimmutable",
        ] {
            if let Some(method) = class.methods.get_mut(name) {
                let returns = ValueType::Class("static".into());
                method.returns = if name == "createfromformat" {
                    returns.join(&ValueType::BoolLiteral(false))
                } else {
                    returns
                };
            }
        }
    }
    if let Some(period) = index.classes.get_mut("dateperiod") {
        let date =
            ValueType::Class("DateTime".into()).join(&ValueType::Class("DateTimeImmutable".into()));
        for method in ["getstartdate", "getenddate"] {
            if let Some(signature) = period.methods.get_mut(method) {
                signature.returns = if method == "getenddate" {
                    date.join(&ValueType::Null)
                } else {
                    date.clone()
                };
            }
        }
    }
}
