//! Evaluate XPath 1.0 expressions over document arena indices.
//!
//! ```php
//! $xpath = new DOMXPath($document);
//! echo $xpath->evaluate('count(//item[@active="yes"])');
//! ```

mod axes;
pub(crate) mod callback;
mod functions;
mod lexer;
mod parser;
mod value;

use std::collections::HashMap;

use crate::tree::{Arena, NodeId};

pub use value::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid,
    Prefix,
    Function,
    Type,
    Arity,
    Limit,
}

impl Error {
    pub fn message(self) -> &'static str {
        match self {
            Self::Invalid => "Invalid expression",
            Self::Prefix => "Undefined namespace prefix",
            Self::Function => "Unregistered function",
            Self::Type => "Invalid type",
            Self::Arity => "Invalid number of arguments",
            Self::Limit => "Recursion limit exceeded",
        }
    }
}

pub fn evaluate(
    arena: &mut Arena,
    context: NodeId,
    namespaces: &HashMap<String, String>,
    expression: &str,
) -> Result<Value, Error> {
    if uses_namespace_axis(expression) {
        arena.prepare_xpath_namespaces(context);
    }
    let expression = parser::parse(expression)?;
    Eval {
        arena,
        namespaces,
        node: context,
        position: 1,
        size: 1,
    }
    .run(&expression, 0)
}

pub(crate) fn uses_namespace_axis(expression: &str) -> bool {
    expression.match_indices("namespace").any(|(start, token)| {
        expression[start + token.len()..]
            .trim_start()
            .starts_with("::")
    })
}

const LIMIT: usize = 64;

#[derive(Clone)]
enum Expr {
    String(String),
    Number(f64),
    Negate(Box<Self>),
    Binary(Operator, Box<Self>, Box<Self>),
    Function(String, Vec<Self>),
    Path {
        base: Option<Box<Self>>,
        absolute: bool,
        steps: Vec<Step>,
    },
    Filter(Box<Self>, Vec<Self>),
}

#[derive(Clone, Copy)]
enum Operator {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Union,
}

#[derive(Clone)]
struct Step {
    axis: Axis,
    test: Test,
    predicates: Vec<Expr>,
}

#[derive(Clone, Copy)]
enum Axis {
    Child,
    Descendant,
    Parent,
    Ancestor,
    FollowingSibling,
    PrecedingSibling,
    Following,
    Preceding,
    Attribute,
    Namespace,
    SelfNode,
    DescendantOrSelf,
    AncestorOrSelf,
}

#[derive(Clone)]
enum Test {
    Any,
    Named(String),
    Node,
    Text,
    Comment,
    ProcessingInstruction(Option<String>),
}

#[derive(Clone, Copy)]
struct Eval<'a> {
    arena: &'a Arena,
    namespaces: &'a HashMap<String, String>,
    node: NodeId,
    position: usize,
    size: usize,
}

impl Eval<'_> {
    fn run(&self, expression: &Expr, depth: usize) -> Result<Value, Error> {
        if depth >= LIMIT {
            return Err(Error::Limit);
        }
        let next = depth + 1;
        match expression {
            Expr::String(value) => Ok(Value::String(value.clone())),
            Expr::Number(value) => Ok(Value::Number(*value)),
            Expr::Negate(expression) => Ok(Value::Number(
                -self.run(expression, next)?.number(self.arena),
            )),
            Expr::Binary(operator, lhs, rhs) => {
                let lhs = self.run(lhs, next)?;
                match operator {
                    Operator::And if !lhs.boolean() => return Ok(Value::Boolean(false)),
                    Operator::Or if lhs.boolean() => return Ok(Value::Boolean(true)),
                    _ => {}
                }
                let rhs = self.run(rhs, next)?;
                self.binary(*operator, lhs, rhs)
            }
            Expr::Function(name, arguments) => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.run(argument, next))
                    .collect::<Result<Vec<_>, _>>()?;
                self.function(name, &arguments)
            }
            Expr::Filter(base, predicates) => {
                let nodes = self.run(base, next)?.nodes()?;
                Ok(Value::Nodes(self.filter(nodes, predicates, next)?))
            }
            Expr::Path {
                base,
                absolute,
                steps,
            } => {
                let mut nodes = if let Some(expression) = base {
                    self.run(expression, next)?.nodes()?
                } else if *absolute {
                    vec![Arena::DOCUMENT]
                } else {
                    vec![self.node]
                };
                for step in steps {
                    let mut output = Vec::new();
                    for node in nodes {
                        let matches = self.axis(node, step)?;
                        output.extend(self.filter(matches, &step.predicates, next)?);
                    }
                    nodes = axes::document_order(self.arena, output);
                }
                Ok(Value::Nodes(nodes))
            }
        }
    }

    fn filter(
        &self,
        mut nodes: Vec<NodeId>,
        predicates: &[Expr],
        depth: usize,
    ) -> Result<Vec<NodeId>, Error> {
        for predicate in predicates {
            let size = nodes.len();
            let mut output = Vec::new();
            for (index, node) in nodes.into_iter().enumerate() {
                let position = index + 1;
                let context = Self {
                    node,
                    position,
                    size,
                    ..*self
                };
                let value = context.run(predicate, depth)?;
                if match value {
                    Value::Number(value) => value == position as f64,
                    value => value.boolean(),
                } {
                    output.push(node);
                }
            }
            nodes = output;
        }
        Ok(nodes)
    }
}

pub fn quote(value: &str) -> String {
    if !value.contains('\'') {
        return format!("'{value}'");
    }
    if !value.contains('"') {
        return format!("\"{value}\"");
    }
    let mut remaining = value;
    let mut pieces = Vec::new();
    while !remaining.is_empty() {
        let quote = match remaining
            .find(['\'', '"'])
            .and_then(|at| remaining.as_bytes().get(at))
        {
            Some(b'\'') => '"',
            _ => '\'',
        };
        let end = remaining.find(quote).unwrap_or(remaining.len());
        let (piece, rest) = remaining.split_at(end);
        pieces.push(format!("{quote}{piece}{quote}"));
        remaining = rest;
    }
    format!("concat({})", pieces.join(","))
}
