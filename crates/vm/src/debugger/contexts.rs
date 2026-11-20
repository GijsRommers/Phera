//! `context_get -i 3 -c 0 -d 0` lists the stopped function's locals.

use super::features::{Features, digit};
use super::properties::Property;
use super::property_xml::render;
use super::protocol::Command;
use crate::Vm;
use phera_value::Value;

const SUPERGLOBALS: &[&str] = &[
    "_COOKIE", "_ENV", "_FILES", "_GET", "_POST", "_REQUEST", "_SERVER", "_SESSION",
];

impl Vm {
    pub(super) async fn debug_properties_command(
        &mut self,
        command: &Command,
        features: &Features,
    ) -> String {
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
        let property = if context == 1 {
            self.debug_global_property(full)
                .and_then(|root| root.find(self, full, features))
        } else {
            roots
                .into_iter()
                .find_map(|root| root.find(self, full, features))
        };
        let Some(property) = property.map(|property| property.named(full)) else {
            return command.error_with_status(300, "can not get property", "break");
        };
        if property.value.is_none() && command.name != "property_set" {
            return command.error_with_status(300, "can not get property", "break");
        }
        let max = command.number::<usize>(b'm').unwrap_or(features.max_data);
        match command.name.as_str() {
            "property_value" => {
                let (kind, size, value) = property.contents(max);
                command.response(
                    &format!(r#" type="{kind}" encoding="base64" size="{size}""#),
                    &value,
                )
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
                let mut limits = features.clone();
                limits.max_data = max;
                command.response("", &render(self, vec![property], &limits, page))
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
                        self.constants.origin(name)?;
                        let value = self.constants.get(name)?.value.clone();
                        Some(Property::new(name.to_owned(), Some(value)).with_facet("constant"))
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    fn debug_global_property(&self, full: &str) -> Option<Property> {
        let end = full.find(['[', '-', ':']).unwrap_or(full.len());
        let root = full.get(..end)?;
        let value = self.request_global_value(root.strip_prefix('$')?)?;
        Some(Property::new(root.to_owned(), Some(value.clone())))
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
