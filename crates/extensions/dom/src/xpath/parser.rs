//! Parse XPath operators, location paths, and predicates.
//!
//! ```php
//! $xpath->query('/root/item[position() <= 2]/@id');
//! ```

use super::lexer::{self, Token};
use super::{Axis, Error, Expr, LIMIT, Operator, Step, Test};

pub(super) fn parse(expression: &str) -> Result<Expr, Error> {
    let tokens = lexer::tokens(expression)?;
    let mut parser = Parser { tokens, index: 0 };
    let result = parser.expression(0, 0)?;
    if parser.peek() != Token::End {
        return Err(Error::Invalid);
    }
    Ok(result)
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    index: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Token<'a> {
        self.tokens.get(self.index).copied().unwrap_or(Token::End)
    }

    fn second(&self) -> Token<'a> {
        self.tokens
            .get(self.index + 1)
            .copied()
            .unwrap_or(Token::End)
    }

    fn take(&mut self) -> Token<'a> {
        let value = self.peek();
        self.index += 1;
        value
    }

    fn accept(&mut self, token: Token<'a>) -> bool {
        if self.peek() == token {
            self.take();
            true
        } else {
            false
        }
    }

    fn require(&mut self, token: Token<'a>) -> Result<(), Error> {
        self.accept(token).then_some(()).ok_or(Error::Invalid)
    }

    fn expression(&mut self, precedence: u8, depth: usize) -> Result<Expr, Error> {
        if depth >= LIMIT {
            return Err(Error::Limit);
        }
        let mut lhs = if self.accept(Token::Minus) {
            Expr::Negate(Box::new(self.expression(7, depth + 1)?))
        } else {
            self.path(depth + 1)?
        };
        let mut operations = 0;
        while let Some((operator, priority)) = binary(self.peek()) {
            if priority < precedence {
                break;
            }
            operations += 1;
            if operations >= LIMIT {
                return Err(Error::Limit);
            }
            self.take();
            let rhs = self.expression(priority + 1, depth + 1)?;
            lhs = Expr::Binary(operator, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn path(&mut self, depth: usize) -> Result<Expr, Error> {
        let mut steps = Vec::new();
        let absolute = matches!(self.peek(), Token::Slash | Token::DoubleSlash);
        let mut base = None;
        if absolute {
            let leading = self.take();
            if leading == Token::DoubleSlash {
                steps.push(descendant());
                steps.push(self.step(depth)?);
            } else if self.starts_step() {
                steps.push(self.step(depth)?);
            }
        } else if self.starts_step() {
            steps.push(self.step(depth)?);
        } else {
            let mut value = match self.take() {
                Token::String(value) => Expr::String(value.into()),
                Token::Number(value) => Expr::Number(value),
                Token::LParen => {
                    let expression = self.expression(0, depth + 1)?;
                    self.require(Token::RParen)?;
                    expression
                }
                Token::Name(name) if self.accept(Token::LParen) => {
                    Expr::Function(name.into(), self.arguments(depth)?)
                }
                _ => return Err(Error::Invalid),
            };
            let predicates = self.predicates(depth)?;
            if !predicates.is_empty() {
                value = Expr::Filter(Box::new(value), predicates);
            }
            if !matches!(self.peek(), Token::Slash | Token::DoubleSlash) {
                return Ok(value);
            }
            base = Some(Box::new(value));
        }
        while matches!(self.peek(), Token::Slash | Token::DoubleSlash) {
            if self.take() == Token::DoubleSlash {
                steps.push(descendant());
            }
            steps.push(self.step(depth)?);
        }
        Ok(Expr::Path {
            base,
            absolute,
            steps,
        })
    }

    fn starts_step(&self) -> bool {
        match self.peek() {
            Token::Dot | Token::DoubleDot | Token::At | Token::Star => true,
            Token::Name(name) => self.second() != Token::LParen || is_node_test(name),
            _ => false,
        }
    }

    fn step(&mut self, depth: usize) -> Result<Step, Error> {
        let (axis, test) = match self.peek() {
            Token::Dot => {
                self.take();
                (Axis::SelfNode, Test::Node)
            }
            Token::DoubleDot => {
                self.take();
                (Axis::Parent, Test::Node)
            }
            _ => {
                let axis = if self.accept(Token::At) {
                    Axis::Attribute
                } else if let Token::Name(name) = self.peek()
                    && self.second() == Token::ColonColon
                {
                    self.take();
                    self.take();
                    parse_axis(name)?
                } else {
                    Axis::Child
                };
                (axis, self.node_test()?)
            }
        };
        Ok(Step {
            axis,
            test,
            predicates: self.predicates(depth)?,
        })
    }

    fn node_test(&mut self) -> Result<Test, Error> {
        match self.take() {
            Token::Star => Ok(Test::Any),
            Token::Name(name) if self.accept(Token::LParen) => {
                let test = match name {
                    "node" => Test::Node,
                    "text" => Test::Text,
                    "comment" => Test::Comment,
                    "processing-instruction" => {
                        let name = if let Token::String(name) = self.peek() {
                            self.take();
                            Some(name.into())
                        } else {
                            None
                        };
                        Test::ProcessingInstruction(name)
                    }
                    _ => return Err(Error::Invalid),
                };
                self.require(Token::RParen)?;
                Ok(test)
            }
            Token::Name(name) if !name.ends_with(':') => Ok(Test::Named(name.into())),
            _ => Err(Error::Invalid),
        }
    }

    fn predicates(&mut self, depth: usize) -> Result<Vec<Expr>, Error> {
        let mut predicates = Vec::new();
        while self.accept(Token::LBracket) {
            predicates.push(self.expression(0, depth + 1)?);
            self.require(Token::RBracket)?;
        }
        Ok(predicates)
    }

    fn arguments(&mut self, depth: usize) -> Result<Vec<Expr>, Error> {
        let mut arguments = Vec::new();
        if self.accept(Token::RParen) {
            return Ok(arguments);
        }
        loop {
            arguments.push(self.expression(0, depth + 1)?);
            if self.accept(Token::RParen) {
                return Ok(arguments);
            }
            self.require(Token::Comma)?;
        }
    }
}

fn is_node_test(name: &str) -> bool {
    matches!(name, "node" | "text" | "comment" | "processing-instruction")
}

fn parse_axis(name: &str) -> Result<Axis, Error> {
    Ok(match name {
        "child" => Axis::Child,
        "descendant" => Axis::Descendant,
        "parent" => Axis::Parent,
        "ancestor" => Axis::Ancestor,
        "following-sibling" => Axis::FollowingSibling,
        "preceding-sibling" => Axis::PrecedingSibling,
        "following" => Axis::Following,
        "preceding" => Axis::Preceding,
        "attribute" => Axis::Attribute,
        "namespace" => Axis::Namespace,
        "self" => Axis::SelfNode,
        "descendant-or-self" => Axis::DescendantOrSelf,
        "ancestor-or-self" => Axis::AncestorOrSelf,
        _ => return Err(Error::Invalid),
    })
}

fn descendant() -> Step {
    Step {
        axis: Axis::DescendantOrSelf,
        test: Test::Node,
        predicates: Vec::new(),
    }
}

fn binary(token: Token<'_>) -> Option<(Operator, u8)> {
    use Operator::*;
    Some(match token {
        Token::Name("or") => (Or, 1),
        Token::Name("and") => (And, 2),
        Token::Equal => (Equal, 3),
        Token::NotEqual => (NotEqual, 3),
        Token::Less => (Less, 4),
        Token::LessEqual => (LessEqual, 4),
        Token::Greater => (Greater, 4),
        Token::GreaterEqual => (GreaterEqual, 4),
        Token::Plus => (Add, 5),
        Token::Minus => (Subtract, 5),
        Token::Star => (Multiply, 6),
        Token::Name("div") => (Divide, 6),
        Token::Name("mod") => (Modulo, 6),
        Token::Pipe => (Union, 8),
        _ => return None,
    })
}
