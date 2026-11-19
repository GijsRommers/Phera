//! `array{id: int, label?: string}` keeps optional keys. Callable docs keep signatures.

use super::*;
use crate::index::{Location, ParameterInfo, Signature};

impl Parser<'_, '_> {
    pub fn shape(&mut self, name: &str, depth: usize) -> Result<ValueType> {
        let kind = match name {
            "list" => ShapeKind::List,
            "object" => ShapeKind::Object,
            _ => ShapeKind::Array,
        };
        let mut fields = Vec::new();
        let mut remainder = None;
        let mut next_key = 0;
        while !self.eat('}') {
            if self.peek() == Some(Token::Dots) {
                self.cursor += 1;
                let args = if self.eat('<') {
                    self.type_list(depth, '>')?
                } else {
                    Vec::new()
                };
                let ty = primitive::collection(CollectionKind::Array, false, args);
                if let ValueType::Collection(collection) = ty {
                    remainder = Some((*collection).clone());
                }
                self.eat(',');
                self.need('}', "'}' after the open shape")?;
                break;
            }
            let explicit = matches!(self.peek_at(1), Some(Token::Punctuation(':' | '?')));
            let (key, optional) = if explicit {
                let key = match self.next() {
                    Some(Token::Word(name)) => name
                        .parse::<i64>()
                        .map_or_else(|_| ShapeKey::String(name.into()), ShapeKey::Integer),
                    Some(Token::Quoted(name)) => ShapeKey::String(name.into()),
                    _ => return Err(self.error("a shape key")),
                };
                let optional = self.eat('?');
                self.need(':', "':' after the shape key")?;
                (key, optional)
            } else {
                (ShapeKey::Integer(next_key), false)
            };
            if let ShapeKey::Integer(index) = key {
                next_key = next_key.max(index.saturating_add(1));
            }
            let value = self.ty(depth)?;
            fields.push(Field {
                key,
                optional,
                value,
            });
            if self.eat('}') {
                break;
            }
            self.need(',', "',' between shape fields")?;
        }
        Ok(ValueType::Shape(Arc::new(Shape {
            fields,
            remainder,
            kind,
        })))
    }

    pub fn callable(&mut self, depth: usize) -> Result<ValueType> {
        let mut signature = Signature::new(
            &[],
            None,
            self.names,
            Location {
                file: String::new(),
                line: 0,
            },
        );
        while !self.eat(')') {
            let untyped_variadic = self.peek() == Some(Token::Dots)
                && matches!(self.peek_at(1), Some(Token::Variable(_)));
            if untyped_variadic {
                self.cursor += 1;
            }
            let ty = if matches!(self.peek(), Some(Token::Variable(name)) if name != "this") {
                ValueType::Mixed
            } else {
                self.ty(depth)?
            };
            let by_reference = self.eat('&');
            let trailing_variadic = self.peek() == Some(Token::Dots);
            if trailing_variadic {
                self.cursor += 1;
            }
            let name = if let Some(Token::Variable(name)) = self.peek() {
                let name = name.to_string();
                self.cursor += 1;
                name
            } else {
                format!("arg{}", signature.parameters.len() + 1)
            };
            let optional = self.eat('=');
            let default_is_null = optional && self.peek() == Some(Token::Word("null"));
            if optional {
                self.skip_default()?;
            }
            signature.parameters.push(ParameterInfo {
                name,
                ty,
                optional,
                default_is_null,
                variadic: untyped_variadic || trailing_variadic,
                by_reference,
                closure_receiver: None,
            });
            if self.eat(')') {
                break;
            }
            self.need(',', "',' between callable parameters")?;
        }
        if self.eat(':') {
            signature.returns = self.ty(depth)?;
        }
        Ok(ValueType::Callable(Arc::new(signature)))
    }

    fn skip_default(&mut self) -> Result<()> {
        // @method takes PHP-style defaults, such as `$flags = [A::OPTION]`.
        // They make the parameter optional without changing its declared type.
        let mut closing = Vec::new();
        while let Some(token) = self.peek() {
            match token {
                Token::Punctuation(',' | ')') if closing.is_empty() => return Ok(()),
                Token::Punctuation('(') => closing.push(')'),
                Token::Punctuation('[') => closing.push(']'),
                Token::Punctuation('{') => closing.push('}'),
                Token::Punctuation(ch @ (')' | ']' | '}')) if closing.pop() != Some(ch) => {
                    return Err(self.error("a balanced parameter default"));
                }
                _ => {}
            }
            self.cursor += 1;
        }
        Err(self.error("',' or ')' after a parameter default"))
    }
}
