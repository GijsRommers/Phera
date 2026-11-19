//! Convert XPath values and implement node-set comparisons.
//!
//! ```php
//! $xpath->evaluate('//item/@id = "first"');
//! ```

use crate::tree::{Arena, Kind, NodeId};

use super::{Error, Eval, Operator, axes};

#[derive(Clone, Debug)]
pub enum Value {
    Nodes(Vec<NodeId>),
    String(String),
    Number(f64),
    Boolean(bool),
}

impl Value {
    pub(super) fn nodes(self) -> Result<Vec<NodeId>, Error> {
        match self {
            Self::Nodes(nodes) => Ok(nodes),
            _ => Err(Error::Type),
        }
    }

    pub(super) fn boolean(&self) -> bool {
        match self {
            Self::Nodes(nodes) => !nodes.is_empty(),
            Self::String(value) => !value.is_empty(),
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::Boolean(value) => *value,
        }
    }

    pub(super) fn number(&self, arena: &Arena) -> f64 {
        match self {
            Self::Number(number) => *number,
            Self::Boolean(value) => f64::from(u8::from(*value)),
            Self::String(value) => number(value),
            Self::Nodes(_) => number(&self.string(arena)),
        }
    }

    pub(crate) fn string(&self, arena: &Arena) -> String {
        match self {
            Self::Number(number) if number.is_nan() => "NaN".into(),
            Self::Number(number) if *number == f64::INFINITY => "Infinity".into(),
            Self::Number(number) if *number == f64::NEG_INFINITY => "-Infinity".into(),
            Self::Number(number) if *number == 0.0 => "0".into(),
            Self::Number(number) => number.to_string(),
            Self::Boolean(value) => value.to_string(),
            Self::String(value) => value.clone(),
            Self::Nodes(nodes) => nodes
                .first()
                .map_or_else(String::new, |&id| string_value(arena, id)),
        }
    }
}

pub(super) fn string_value(arena: &Arena, id: NodeId) -> String {
    if matches!(
        arena[id].kind,
        Kind::Element | Kind::Document | Kind::Fragment
    ) {
        let mut value = String::new();
        for child in arena.descendants(id) {
            if matches!(
                arena[child].kind,
                Kind::Text | Kind::Cdata | Kind::EntityReference
            ) {
                value.push_str(&arena[child].value);
            }
        }
        value
    } else {
        arena[id].value.clone()
    }
}

pub(super) fn number(string: &str) -> f64 {
    let string = string.trim_matches([' ', '\t', '\r', '\n']);
    let content = string.strip_prefix('-').unwrap_or(string);
    if content
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        string.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

impl Eval<'_> {
    pub(super) fn binary(&self, op: Operator, lhs: Value, rhs: Value) -> Result<Value, Error> {
        use Operator::*;
        Ok(match op {
            Or => Value::Boolean(lhs.boolean() || rhs.boolean()),
            And => Value::Boolean(lhs.boolean() && rhs.boolean()),
            Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual => {
                Value::Boolean(self.compare(op, &lhs, &rhs))
            }
            Add => Value::Number(lhs.number(self.arena) + rhs.number(self.arena)),
            Subtract => Value::Number(lhs.number(self.arena) - rhs.number(self.arena)),
            Multiply => Value::Number(lhs.number(self.arena) * rhs.number(self.arena)),
            Divide => Value::Number(lhs.number(self.arena) / rhs.number(self.arena)),
            Modulo => Value::Number(lhs.number(self.arena) % rhs.number(self.arena)),
            Union => {
                let mut nodes = lhs.nodes()?;
                nodes.extend(rhs.nodes()?);
                Value::Nodes(axes::document_order(self.arena, nodes))
            }
        })
    }

    fn compare(&self, op: Operator, lhs: &Value, rhs: &Value) -> bool {
        use Operator::*;
        let equal = matches!(op, Equal | NotEqual);
        let arena = self.arena;
        let numeric = |left: f64, right: f64| match op {
            Equal => left == right,
            NotEqual => left != right,
            Less => left < right,
            LessEqual => left <= right,
            Greater => left > right,
            GreaterEqual => left >= right,
            _ => false,
        };
        let strings = |left: &str, right: &str| {
            if equal {
                (left == right) == matches!(op, Equal)
            } else {
                numeric(number(left), number(right))
            }
        };
        match (lhs, rhs) {
            (Value::Nodes(left), Value::Nodes(right)) => left.iter().any(|&left| {
                let left = string_value(arena, left);
                right
                    .iter()
                    .any(|&right| strings(&left, &string_value(arena, right)))
            }),
            (Value::Boolean(_), _) | (_, Value::Boolean(_)) if equal => {
                (lhs.boolean() == rhs.boolean()) == matches!(op, Equal)
            }
            (Value::Nodes(nodes), Value::Number(_) | Value::Boolean(_)) => nodes
                .iter()
                .any(|&node| numeric(number(&string_value(arena, node)), rhs.number(arena))),
            (Value::Number(_) | Value::Boolean(_), Value::Nodes(nodes)) => nodes
                .iter()
                .any(|&node| numeric(lhs.number(arena), number(&string_value(arena, node)))),
            (Value::Nodes(nodes), Value::String(right)) => nodes
                .iter()
                .any(|&node| strings(&string_value(arena, node), right)),
            (Value::String(left), Value::Nodes(nodes)) => nodes
                .iter()
                .any(|&node| strings(left, &string_value(arena, node))),
            (Value::String(left), Value::String(right)) => strings(left, right),
            _ => numeric(lhs.number(arena), rhs.number(arena)),
        }
    }
}
