//! `stack_get -i 8` lists suspended frames. `context_get -i 9 -d 0 -c 0` lists locals.

use std::fmt::Write;

use crate::Vm;

use super::features::Features;
use super::properties::Property;
use super::property_xml::render;
use super::protocol::Command;
use super::source::source;

impl Vm {
    pub(super) async fn debug_inspection(
        &mut self,
        command: &Command,
        features: &Features,
        file: &str,
        line: Option<u32>,
    ) -> String {
        match command.name.as_str() {
            "stack_depth" => {
                command.response(&format!(r#" depth="{}""#, self.call_stack.depth()), "")
            }
            "stack_get" => self.debug_stack(command, line),
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
                        "",
                        &render(
                            self,
                            vec![Property::new(String::new(), Some(value))],
                            features,
                            0,
                        ),
                    ),
                    Err(_) => command.error_with_status(206, "error evaluating code", "break"),
                }
            }
            "source" => source(command, file).await,
            _ => command.error(4, "unimplemented command"),
        }
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
