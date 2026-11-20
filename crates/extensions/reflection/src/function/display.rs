//! `(string) new ReflectionFunction('json_decode')` shows native parameter defaults.

use std::fmt::Write;

use phera_extension_api::reflection::{FunctionInfo, ParameterInfo};

pub(super) fn function(function: &FunctionInfo) -> String {
    let Some(extension) = &function.extension else {
        return format!("Function [ {} ]\n", function.name);
    };
    let deprecated = if function.deprecated {
        ", deprecated"
    } else {
        ""
    };
    let reference = if function.returns_reference { "&" } else { "" };
    let mut text = format!(
        "Function [ <internal{deprecated}:{extension}> function {reference}{} ] {{\n\n  - Parameters [{}] {{\n",
        function.name,
        function.parameters.len(),
    );
    for parameter in &function.parameters {
        let _ = writeln!(text, "    {}", display_parameter(parameter));
    }
    text.push_str("  }\n");
    if let Some(ty) = &function.return_type {
        let _ = writeln!(text, "  - Return [ {ty} ]");
    }
    text.push_str("}\n");
    text
}

pub(crate) fn display_parameter(parameter: &ParameterInfo) -> String {
    let required = if parameter.optional {
        "optional"
    } else {
        "required"
    };
    let mut text = format!("Parameter #{} [ <{required}> ", parameter.position);
    if let Some(ty) = &parameter.declared_type {
        let _ = write!(text, "{ty} ");
    }
    if parameter.by_reference {
        text.push('&');
    }
    if parameter.variadic {
        text.push_str("...");
    }
    let _ = write!(text, "${}", parameter.name);
    if parameter.optional && !parameter.variadic {
        let default = parameter.default_display.as_deref().unwrap_or("<default>");
        let _ = write!(text, " = {default}");
    }
    text.push_str(" ]");
    text
}
