//! `stack_get -i 8` lists suspended frames. `context_get -i 9 -d 0 -c 0` lists locals.

use std::fmt::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_value::Value;
use quick_xml::escape::escape;

use crate::Vm;

use super::features::{Features, digit};
use super::properties::{Property, render};
use super::protocol::{Command, file_uri};

const SUPERGLOBALS: &[&str] = &[
    "_COOKIE", "_ENV", "_FILES", "_GET", "_POST", "_REQUEST", "_SERVER", "_SESSION",
];

impl Vm {
    pub(super) async fn debug_inspection(
        &mut self,
        command: &Command,
        features: &Features,
        file: &str,
    ) -> String {
        match command.name.as_str() {
            "stack_depth" => {
                command.response(&format!(r#" depth="{}""#, self.call_stack.depth()), "")
            }
            "stack_get" => self.debug_stack(command),
            "context_names" => command.response(
                "",
                concat!(
                    r#"<context name="Locals" id="0"/>"#,
                    r#"<context name="Superglobals" id="1"/>"#,
                    r#"<context name="User defined constants" id="2"/>"#,
                ),
            ),
            "context_get" | "property_get" | "property_value" | "property_set" => {
                self.debug_properties_command(command, features).await
            }
            "typemap_get" => typemap(command),
            "eval" => {
                let Some(expression) = command.data() else {
                    return command.error(3, "missing expression");
                };
                match self.debug_eval(expression).await {
                    Ok(value) => command.response(
                        r#" success="1""#,
                        &render(
                            self,
                            vec![Property::new(String::new(), Some(value))],
                            features,
                            0,
                        ),
                    ),
                    Err(_) => command.error(206, "error evaluating code"),
                }
            }
            "source" => source(command, file).await,
            _ => command.error(4, "unimplemented command"),
        }
    }

    fn debug_stack(&self, command: &Command) -> String {
        let mut body = String::new();
        let requested = command.number::<usize>(b'd');
        let frames = self.frames();
        let depth = frames.len();
        for (index, frame) in frames.rev().enumerate() {
            if requested.is_some_and(|selected| selected != index) {
                continue;
            }
            let file = file_uri(&frame.unit().file);
            let line = if index == 0 {
                frame
                    .function()
                    .lines
                    .get(frame.ip())
                    .copied()
                    .map_or(frame.line(), |line| line as usize)
            } else {
                frame.line()
            };
            let name = if frame.function_id().is_none() && index + 1 < depth {
                depth
                    .checked_sub(index + 2)
                    .and_then(|caller| self.frame_at(caller))
                    .and_then(crate::exception_traces::scoped_unit_name)
                    .unwrap_or("{main}")
                    .to_owned()
            } else {
                let name = self.code.function_name(frame.frame.function);
                if frame.this().is_some() && !name.starts_with("{closure:") {
                    name.replacen("::", "->", 1)
                } else {
                    name.to_owned()
                }
            };
            let _ = write!(
                body,
                r#"<stack where="{}" level="{index}" type="file" filename="{}" lineno="{line}"/>"#,
                escape(&name),
                escape(&file)
            );
        }
        if body.is_empty() && requested.is_some() {
            return command.error(301, "invalid stack depth");
        }
        command.response("", &body)
    }

    async fn debug_properties_command(&mut self, command: &Command, features: &Features) -> String {
        let depth = command.number::<usize>(b'd').unwrap_or_default();
        let Some(frame) = self.call_stack.depth().checked_sub(depth.saturating_add(1)) else {
            return command.error(301, "invalid stack depth");
        };
        let context = command.number::<u8>(b'c').unwrap_or_default();
        let Some(roots) = self.debug_context(frame, context) else {
            return command.error(302, "invalid context");
        };
        if command.name == "context_get" {
            return command.response(
                &format!(r#" context="{context}""#),
                &render(self, roots, features, 0),
            );
        }
        let Some(full) = command.get(b'n') else {
            return command.error(3, "missing property name");
        };
        let Some(property) = roots.into_iter().find_map(|root| root.find(self, full)) else {
            return command.error(300, "property does not exist");
        };
        let max = command.number::<usize>(b'm').unwrap_or(features.max_data);
        match command.name.as_str() {
            "property_value" => {
                let (size, value) = property.contents(max);
                command.response(&format!(r#" encoding="base64" size="{size}""#), &value)
            }
            "property_set" => {
                let Some(value) = command.data() else {
                    return command.error(3, "missing property value");
                };
                let success = self
                    .debug_property_set(frame, depth, context, full, command.get(b't'), value)
                    .await;
                command.response(&format!(r#" success="{}""#, digit(success)), "")
            }
            _ => {
                let page = command.number(b'p').unwrap_or_default();
                command.response("", &render(self, vec![property], features, page))
            }
        }
    }

    fn debug_context(&self, frame: usize, context: u8) -> Option<Vec<Property>> {
        let frame = self.frame_at(frame)?;
        match context {
            0 => {
                let mut variables: Vec<_> = frame
                    .named_registers()
                    .filter_map(|(name, register)| {
                        if SUPERGLOBALS.contains(&name.as_str()) {
                            return None;
                        }
                        let value = frame
                            .slots(&self.registers)
                            .get(register.index())
                            .cloned()
                            .flatten();
                        Some(Property::new(format!("${name}"), value))
                    })
                    .collect();
                if let Some(object) = frame.this() {
                    variables.push(Property::new(
                        "$this".into(),
                        Some(Value::Object(object.clone())),
                    ));
                }
                variables.sort_unstable_by(|left, right| left.full.cmp(&right.full));
                Some(variables)
            }
            1 => Some(
                SUPERGLOBALS
                    .iter()
                    .filter_map(|name| {
                        let value = self.request_global_value(name)?.clone();
                        Some(
                            Property::new(format!("${name}"), Some(value))
                                .with_facet("superglobal"),
                        )
                    })
                    .collect(),
            ),
            2 => Some(
                self.constants
                    .user_names()
                    .filter_map(|name| {
                        let value = self.constants.get(name)?.value.clone();
                        Some(Property::new(name.to_owned(), Some(value)).with_facet("constant"))
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    async fn debug_property_set(
        &mut self,
        frame: usize,
        depth: usize,
        context: u8,
        full: &str,
        kind: Option<&str>,
        data: &[u8],
    ) -> bool {
        if context == 2 || full == "$this" {
            return false;
        }
        if depth == 0 && kind.is_none() {
            let mut expression = format!("{full} = (").into_bytes();
            expression.extend_from_slice(data);
            expression.extend_from_slice(b"\n)");
            return self.debug_eval(&expression).await.is_ok();
        }
        let value = match kind {
            Some("string") => Some(Value::from(phera_value::PhpString::from(data.to_vec()))),
            Some("null") => Some(Value::Null),
            Some("bool") => Some(Value::Bool(data != b"0" && !data.is_empty())),
            Some("int") => std::str::from_utf8(data)
                .ok()
                .and_then(|text| text.parse().ok())
                .map(Value::Int),
            Some("float") => std::str::from_utf8(data)
                .ok()
                .and_then(|text| text.parse().ok())
                .map(Value::Float),
            Some(_) => None,
            None => self.debug_eval(data).await.ok(),
        };
        let Some(value) = value else { return false };
        let Some(name) = full.strip_prefix('$') else {
            return false;
        };
        let Some(view) = self.frame_at(frame) else {
            return false;
        };
        let Some(register) = view.named_register(name) else {
            return false;
        };
        if depth == 0 {
            return self.assign_local(register, value).is_ok();
        }
        let position = view.base() + register.index();
        let reference = self
            .registers
            .window(position, 1)
            .first()
            .and_then(Option::as_ref)
            .and_then(|value| match value {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            });
        if let Some(reference) = reference {
            let Ok(value) = self.check_reference_assignment(&reference, value) else {
                return false;
            };
            let Ok(mut cell) = reference.try_borrow_mut() else {
                return false;
            };
            *cell = value;
        } else if let Some(slot) = self.registers.window_mut(position, 1).first_mut() {
            *slot = Some(value);
        } else {
            return false;
        }
        true
    }
}

fn typemap(command: &Command) -> String {
    let mut body = String::new();
    for (name, kind, schema) in [
        ("bool", "bool", "boolean"),
        ("int", "int", "decimal"),
        ("float", "float", "double"),
        ("string", "string", "string"),
        ("null", "null", ""),
        ("array", "hash", ""),
        ("object", "object", ""),
        ("resource", "resource", ""),
        ("uninitialized", "uninitialized", ""),
    ] {
        let _ = write!(body, r#"<map type="{kind}" name="{name}""#);
        if !schema.is_empty() {
            let _ = write!(body, r#" xsi:type="xsd:{schema}""#);
        }
        body.push_str("/>");
    }
    command.response(
        r#" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema""#,
        &body,
    )
}

async fn source(command: &Command, fallback: &str) -> String {
    let file = command.get(b'f').unwrap_or(fallback);
    let Some(path) = url::Url::parse(file)
        .ok()
        .and_then(|uri| uri.to_file_path().ok())
    else {
        return command.error(100, "could not open source");
    };
    let Ok(bytes) = tokio::fs::read(path).await else {
        return command.error(100, "could not open source");
    };
    let first = command.number::<usize>(b'b').unwrap_or(1).max(1);
    let last = command.number::<usize>(b'e').unwrap_or(usize::MAX);
    let range: Vec<u8> = bytes
        .split_inclusive(|byte| *byte == b'\n')
        .skip(first - 1)
        .take(last.saturating_add(1).saturating_sub(first))
        .flatten()
        .copied()
        .collect();
    command.response(r#" encoding="base64""#, &STANDARD.encode(range))
}
