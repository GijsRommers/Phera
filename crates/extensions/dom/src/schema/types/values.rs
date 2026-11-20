//! Check lexical values and simple-type restrictions in the supported XSD subset.
//!
//! ```php
//! $document->schemaValidateSource($schema); // checks a declared xs:int value
//! ```

use super::{Schema, Type};

impl Schema {
    pub(super) fn valid_value(&self, mut ty: Type, value: &str) -> bool {
        for _ in 0..64 {
            let rule = match ty {
                Type::Builtin(name) => return builtin(&name, value),
                Type::Declared(rule) => rule,
            };
            let Some(restriction) = self
                .children(rule)
                .find(|&child| self.is(child, "restriction"))
            else {
                return false;
            };
            let mut enumerated = false;
            let mut matches_enum = false;
            let mut patterns = false;
            let mut matches_pattern = false;
            for facet in self.children(restriction) {
                let Some(operand) = self.attr(facet, "value") else {
                    continue;
                };
                match self.node(facet).name.local.as_str() {
                    "enumeration" => {
                        enumerated = true;
                        matches_enum |= value == operand;
                    }
                    "pattern" => {
                        patterns = true;
                        matches_pattern |= regex::RegexBuilder::new(&format!(r"\A(?:{operand})\z"))
                            .size_limit(1024 * 1024)
                            .build()
                            .is_ok_and(|regex| regex.is_match(value));
                    }
                    "length" | "minLength" | "maxLength" => {
                        let Ok(limit) = operand.parse::<usize>() else {
                            return false;
                        };
                        let length = value.chars().count();
                        if self.is(facet, "length") && length != limit
                            || self.is(facet, "minLength") && length < limit
                            || self.is(facet, "maxLength") && length > limit
                        {
                            return false;
                        }
                    }
                    "minInclusive" | "maxInclusive" | "minExclusive" | "maxExclusive" => {
                        let (Ok(value), Ok(limit)) =
                            (value.trim().parse::<f64>(), operand.parse::<f64>())
                        else {
                            return false;
                        };
                        let valid = match self.node(facet).name.local.as_str() {
                            "minInclusive" => value >= limit,
                            "maxInclusive" => value <= limit,
                            "minExclusive" => value > limit,
                            _ => value < limit,
                        };
                        if !valid {
                            return false;
                        }
                    }
                    "whiteSpace" => {}
                    _ => return false,
                }
            }
            if enumerated && !matches_enum || patterns && !matches_pattern {
                return false;
            }
            let Some(base) = self
                .attr(restriction, "base")
                .and_then(|base| self.resolve_type(self.qname(restriction, base)))
            else {
                return false;
            };
            ty = base;
        }
        false
    }
}

pub(super) fn xml_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\r' | '\n')
}

fn builtin(kind: &str, value: &str) -> bool {
    let value = value.trim_matches(xml_space);
    match kind {
        "anyType" | "anySimpleType" | "string" | "normalizedString" | "token" | "anyURI" => true,
        "boolean" => matches!(value, "true" | "false" | "0" | "1"),
        "int" => value.parse::<i32>().is_ok(),
        "long" => value.parse::<i64>().is_ok(),
        "short" => value.parse::<i16>().is_ok(),
        "byte" => value.parse::<i8>().is_ok(),
        "unsignedLong" => value.parse::<u64>().is_ok(),
        "unsignedInt" => value.parse::<u32>().is_ok(),
        "unsignedShort" => value.parse::<u16>().is_ok(),
        "unsignedByte" => value.parse::<u8>().is_ok(),
        "integer" | "nonNegativeInteger" | "negativeInteger" | "nonPositiveInteger"
        | "positiveInteger" => integer(kind, value),
        "decimal" => !value.contains(['e', 'E']) && value.parse::<f64>().is_ok_and(f64::is_finite),
        "float" | "double" => {
            value.parse::<f64>().is_ok() || matches!(value, "INF" | "-INF" | "NaN")
        }
        "NCName" | "ID" | "IDREF" => !value.contains(':') && crate::xml::valid_name(value),
        "Name" | "QName" => crate::xml::valid_name(value),
        _ => false,
    }
}

fn integer(kind: &str, value: &str) -> bool {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let zero = digits.bytes().all(|byte| byte == b'0');
    let negative = !zero && value.starts_with('-');
    match kind {
        "positiveInteger" => !negative && !zero,
        "nonNegativeInteger" => !negative,
        "negativeInteger" => negative,
        "nonPositiveInteger" => negative || zero,
        _ => true,
    }
}
