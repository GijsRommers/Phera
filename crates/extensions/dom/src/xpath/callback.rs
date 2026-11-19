//! Evaluate callback-bearing XPath expressions without retaining a PHP borrow.
//!
//! ```php
//! $xpath->registerNamespace('php', 'http://php.net/xpath');
//! $xpath->registerPhpFunctions('strtoupper');
//! $xpath->evaluate('php:functionString("strtoupper", "text")');
//! ```

use phera_extension_api::{LocalFuture, NativeError};

use crate::tree::{Arena, NodeId};

use super::{Error, Eval, Expr, LIMIT, Operator, Value, axes, parser};

pub(crate) enum Failure {
    XPath(Error),
    Php(NativeError),
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::XPath(error)
    }
}

pub(crate) trait Callback {
    fn call<'a>(
        &'a mut self,
        name: &'a str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, Result<Value, Failure>>;
}

pub(crate) async fn evaluate(
    arena: &Arena,
    context: NodeId,
    namespaces: &std::collections::HashMap<String, String>,
    source: &str,
    callback: &mut dyn Callback,
) -> Result<Value, Failure> {
    let expression = parser::parse(source)?;
    let evaluator = Eval {
        arena,
        namespaces,
        node: context,
        position: 1,
        size: 1,
    };
    evaluator.run_async(&expression, 0, callback).await
}

impl Eval<'_> {
    fn run_async<'a>(
        self,
        expression: &'a Expr,
        depth: usize,
        callback: &'a mut dyn Callback,
    ) -> LocalFuture<'a, Result<Value, Failure>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            if depth >= LIMIT {
                return Err(Error::Limit.into());
            }
            let next = depth + 1;
            Ok(match expression {
                Expr::String(value) => Value::String(value.clone()),
                Expr::Number(value) => Value::Number(*value),
                Expr::Negate(value) => Value::Number(
                    -self
                        .run_async(value, next, callback)
                        .await?
                        .number(self.arena),
                ),
                Expr::Binary(op, lhs, rhs) => {
                    let lhs = self.run_async(lhs, next, callback).await?;
                    match op {
                        Operator::And if !lhs.boolean() => return Ok(Value::Boolean(false)),
                        Operator::Or if lhs.boolean() => return Ok(Value::Boolean(true)),
                        _ => {}
                    }
                    let rhs = self.run_async(rhs, next, callback).await?;
                    self.binary(*op, lhs, rhs)?
                }
                Expr::Function(name, arguments) => {
                    let mut values = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        values.push(self.run_async(argument, next, callback).await?);
                    }
                    if name.contains(':') {
                        callback.call(name, values).await?
                    } else {
                        self.function(name, &values)?
                    }
                }
                Expr::Filter(base, predicates) => {
                    let nodes = self.run_async(base, next, callback).await?.nodes()?;
                    Value::Nodes(self.filter_async(nodes, predicates, next, callback).await?)
                }
                Expr::Path {
                    base,
                    absolute,
                    steps,
                } => {
                    let mut nodes = if let Some(expression) = base {
                        self.run_async(expression, next, callback).await?.nodes()?
                    } else if *absolute {
                        vec![Arena::DOCUMENT]
                    } else {
                        vec![self.node]
                    };
                    for step in steps {
                        let mut output = Vec::new();
                        for node in nodes {
                            let matches = self.axis(node, step)?;
                            output.extend(
                                self.filter_async(matches, &step.predicates, next, callback)
                                    .await?,
                            );
                        }
                        nodes = axes::document_order(self.arena, output);
                    }
                    Value::Nodes(nodes)
                }
            })
        })
    }

    async fn filter_async(
        self,
        mut nodes: Vec<NodeId>,
        predicates: &[Expr],
        depth: usize,
        callback: &mut dyn Callback,
    ) -> Result<Vec<NodeId>, Failure> {
        for predicate in predicates {
            let size = nodes.len();
            let mut output = Vec::new();
            for (index, node) in nodes.into_iter().enumerate() {
                let position = index + 1;
                let context = Self {
                    node,
                    position,
                    size,
                    ..self
                };
                let value = context.run_async(predicate, depth, callback).await?;
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
