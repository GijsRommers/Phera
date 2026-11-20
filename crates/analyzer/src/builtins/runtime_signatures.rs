//! `class InvoiceDate extends DateTime {}` inherits factories that create InvoiceDates.
//! Some php-src signatures name the base class even though PHP uses the called class.

use crate::{index::Index, types::ValueType};

pub fn refine(index: &mut Index) {
    preserve_false_return(index);
    // These take either an array or Traversable, as in php-src's `iterable`
    // declarations. Older generated tables stored only the array alternative.
    for name in ["iterator_to_array", "iterator_count"] {
        if let Some(parameter) = index
            .functions
            .get_mut(name)
            .and_then(|signature| signature.parameters.first_mut())
        {
            parameter.ty = ValueType::Iterable;
        }
    }
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

fn preserve_false_return(index: &mut Index) {
    // The committed table widened literal false to bool for these APIs.
    for name in [
        "strpos",
        "strrpos",
        "stripos",
        "strripos",
        "strstr",
        "stristr",
        "strpbrk",
        "strrchr",
        "mb_strpos",
        "mb_strrpos",
        "mb_stripos",
        "mb_strripos",
        "mb_strstr",
        "mb_stristr",
        "mb_strrchr",
        "mb_strrichr",
        "imagecreatefromstring",
        "imagecreatetruecolor",
        "imagecreate",
        "ob_get_contents",
        "ob_get_clean",
        "iconv",
        "parse_ini_file",
        "parse_ini_string",
    ] {
        if let Some(signature) = index.functions.get_mut(name)
            && let ValueType::Union(arms) = &signature.returns
        {
            signature.returns = ValueType::Union(
                arms.iter()
                    .map(|arm| {
                        if *arm == ValueType::Bool {
                            ValueType::BoolLiteral(false)
                        } else {
                            arm.clone()
                        }
                    })
                    .collect(),
            );
        }
    }
}
