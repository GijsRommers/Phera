//! `#[Deprecated(message: 'Use newApi', since: '2')]` reports at the call site.

use crate::{Vm, diagnostics::SourceLocation};
use phera_bytecode::reflection::{Attribute, Initializer};
use phera_bytecode::{CallResult, Instruction};
use phera_extension_api::NativeError;

impl Vm {
    pub(crate) fn function_attribute_diagnostics(&mut self) -> Result<(), NativeError> {
        let Some(frame) = self.current() else {
            return Ok(());
        };
        let depth = self.call_stack.depth();
        let caller = depth.checked_sub(2).and_then(|index| self.frame_at(index));
        let location = caller.map(|caller| SourceLocation {
            file: caller.unit().file.clone(),
            line: caller.line(),
        });
        let discard = caller.is_some_and(|caller| {
            matches!(
                caller
                    .function()
                    .instructions
                    .get(caller.ip().saturating_sub(1)),
                Some(
                    Instruction::Call {
                        result: CallResult::Discarded,
                        ..
                    } | Instruction::CallDirect {
                        result: CallResult::Discarded,
                        ..
                    }
                )
            )
        });
        let name = self.code.function_name(frame.frame.function);
        let is_method = name.contains("::") && !name.starts_with("{closure:");
        let kind = if is_method { "Method" } else { "Function" };
        let Some(metadata) = frame
            .function_id()
            .and_then(|index| frame.unit().function_metadata.get(index))
        else {
            return Ok(());
        };
        let attrs = &metadata.declaration.attributes;
        let deprecated = attribute(attrs, "Deprecated")
            .map(|attr| deprecation_message(attr, &format!("{kind} {name}()")));
        let nodiscard = if discard {
            attribute(attrs, "NoDiscard").map(|attr| {
                let kind = kind.to_ascii_lowercase();
                let mut text = format!(
                    "The return value of {kind} {name}() should either be used or intentionally ignored by casting it as (void)"
                );
                if let Some(message) = attribute_literal_string(attr, "message", 0) {
                    text.push_str(", ");
                    text.push_str(&message);
                }
                text
            })
        } else {
            None
        };
        let previous = std::mem::replace(&mut self.diagnostic_location, location);
        if let Some(message) = deprecated {
            self.attribute_deprecated(message);
        }
        if let Some(message) = nodiscard {
            self.warn(message);
        }
        self.diagnostic_location = previous;
        Ok(())
    }
    fn attribute_deprecated(&mut self, message: String) {
        let event = crate::error_handlers::DiagnosticEvent {
            level: 16384,
            kind: "Deprecated".into(),
            message: message.into(),
            location: self.location(),
            trace_location: None,
            native: None,
        };
        if self.handles_diagnostic(event.level) {
            self.enqueue_diagnostic(event);
        } else {
            self.report_event(event);
        }
    }
}

pub(crate) fn deprecation_message(attr: &Attribute, target: &str) -> String {
    let mut description = format!("{target} is deprecated");
    if let Some(since) = attribute_literal_string(attr, "since", 1) {
        description.push_str(" since ");
        description.push_str(&since);
    }
    if let Some(message) = attribute_literal_string(attr, "message", 0) {
        description.push_str(", ");
        description.push_str(&message);
    }
    description
}

fn attribute_literal_string(attr: &Attribute, name: &str, index: usize) -> Option<String> {
    let arg = attr
        .arguments
        .iter()
        .enumerate()
        .find(|(position, argument)| {
            argument
                .name
                .as_deref()
                .map_or(*position == index, |argument| argument == name)
        })?
        .1;
    match &arg.initializer {
        Initializer::String(value) => Some(String::from_utf8_lossy(value).into_owned()),
        _ => None,
    }
}

pub(crate) fn attribute<'a>(attributes: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))
}
