//! Apply XMLWriter methods and procedural aliases to the same writer.
//!
//! ```php
//! xmlwriter_start_element($writer, 'root');
//! $writer->endElement();
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::bytes::nul_prefix;
use crate::dtd::{self, Declaration};
use crate::output;
use crate::registration::{BINDINGS, Binding};
use crate::writer::{Frame, Writer};

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Indent,
    IndentString,
    StartComment,
    EndComment,
    StartAttribute,
    EndAttribute,
    WriteAttribute,
    StartAttributeNs,
    WriteAttributeNs,
    StartElement,
    EndElement,
    FullEndElement,
    StartElementNs,
    WriteElement,
    WriteElementNs,
    StartPi,
    EndPi,
    WritePi,
    StartCdata,
    EndCdata,
    WriteCdata,
    Text,
    WriteRaw,
    StartDocument,
    EndDocument,
    WriteComment,
    StartDtd,
    EndDtd,
    WriteDtd,
    StartDtdElement,
    EndDtdElement,
    WriteDtdElement,
    StartDtdAttlist,
    EndDtdAttlist,
    WriteDtdAttlist,
    StartDtdEntity,
    EndDtdEntity,
    WriteDtdEntity,
    OutputMemory,
    Flush,
}

pub fn method<const INDEX: usize>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let binding = &BINDINGS[INDEX];
        let object = this.ok_or_else(output::uninitialized)?;
        invoke(context, &object, &arguments, binding, true).await
    })
}

pub fn function<const INDEX: usize>(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let binding = &BINDINGS[INDEX];
        let object = args::object(&arguments, 0).ok_or_else(output::uninitialized)?;
        invoke(context, object, &arguments[1..], binding, false).await
    })
}

async fn invoke(
    context: &mut dyn Context,
    object: &Object,
    arguments: &[Value],
    binding: &Binding,
    method: bool,
) -> NativeResult {
    let state = output::state(object)?;
    let called = if method {
        format!("XMLWriter::{}", binding.method.name)
    } else {
        binding.function.name.to_owned()
    };
    let operation = binding.operation;
    if matches!(operation, Operation::OutputMemory | Operation::Flush) {
        return output::flush(
            context,
            &state,
            &called,
            operation == Operation::OutputMemory,
            args::bool(arguments, 0).unwrap_or(true),
        )
        .await;
    }
    validate_name(binding, arguments, &called, method)?;
    let result = apply(&mut state.borrow_mut().writer, operation, arguments);
    let result = match result {
        Ok(result) => result,
        Err(message) => {
            context.warning(format!("{called}(): {message}"));
            false
        }
    };
    let flush = {
        let state = state.borrow();
        operation == Operation::EndDocument || state.writer.output.len() >= output::BUFFER_SIZE
    };
    if flush {
        let result_count = output::publish(context, &state, &called).await;
        return Ok(Value::Bool(result && result_count >= 0));
    }
    Ok(Value::Bool(result))
}

fn validate_name(
    binding: &Binding,
    arguments: &[Value],
    called: &str,
    method: bool,
) -> Result<(), NativeError> {
    use Operation::*;
    let (index, description) = match binding.operation {
        StartAttribute | WriteAttribute | StartDtdEntity => (0, "attribute name"),
        StartAttributeNs | WriteAttributeNs => (1, "attribute name"),
        StartElementNs | WriteElementNs => (1, "element name"),
        StartElement | WriteElement | StartDtdElement | WriteDtdElement | StartDtdAttlist
        | WriteDtdAttlist | WriteDtdEntity => (0, "element name"),
        StartPi | WritePi => (0, "PI target"),
        _ => return Ok(()),
    };
    let name = nul_prefix(args::bytes(arguments, index));
    if std::str::from_utf8(name).is_ok_and(phera_ext_dom::xml::valid_name) {
        return Ok(());
    }
    // PHP's shared implementation includes its procedural writer argument
    // when reporting a ValueError from a method.
    let params = if method {
        binding.method.params
    } else {
        binding.function.params
    };
    let php_index = index + 1;
    let parameter = params
        .get(php_index)
        .map(|param| format!(" (${})", param.name))
        .unwrap_or_default();
    Err(NativeError::value_error(format!(
        "{called}(): Argument #{}{parameter} must be a valid {description}, \"{}\" given",
        php_index + 1,
        String::from_utf8_lossy(name),
    )))
}

fn apply(
    writer: &mut Writer,
    operation: Operation,
    arguments: &[Value],
) -> Result<bool, &'static str> {
    use Operation::*;
    let bytes = |index| nul_prefix(args::bytes(arguments, index));
    let optional = |index| args::string(arguments, index).map(|text| nul_prefix(text.as_bytes()));
    let enabled = |index| args::bool(arguments, index).unwrap_or(false);
    let pi = || Frame::Pi { started: false };
    let result = match operation {
        Indent => {
            writer.indent = enabled(0);
            true
        }
        IndentString => {
            writer.padding = bytes(0).to_vec();
            true
        }
        StartComment => writer.start_section(Frame::Comment, b""),
        EndComment => writer.end_section(Frame::Comment),
        StartAttribute => writer.start_attribute(None, bytes(0), None),
        EndAttribute => writer.end_attribute(),
        WriteAttribute => {
            let opened = writer.start_attribute(None, bytes(0), None);
            write_body(writer, opened, Some(bytes(1)), Writer::end_attribute)
        }
        StartAttributeNs => writer.start_attribute(optional(0), bytes(1), optional(2)),
        WriteAttributeNs => {
            let opened = writer.start_attribute(optional(0), bytes(1), optional(2));
            write_body(writer, opened, Some(bytes(3)), Writer::end_attribute)
        }
        StartElement => writer.start_element(None, bytes(0), None),
        EndElement => writer.end_element(false),
        FullEndElement => writer.end_element(true),
        StartElementNs => writer.start_element(optional(0), bytes(1), optional(2)),
        WriteElement => {
            let opened = writer.start_element(None, bytes(0), None);
            write_body(writer, opened, optional(1), |writer| {
                writer.end_element(false)
            })
        }
        WriteElementNs => {
            let opened = writer.start_element(optional(0), bytes(1), optional(2));
            write_body(writer, opened, optional(3), |writer| {
                writer.end_element(false)
            })
        }
        StartPi => writer.start_section(pi(), bytes(0)),
        EndPi => writer.end_section(pi()),
        WritePi => {
            let opened = writer.start_section(pi(), bytes(0));
            write_body(writer, opened, Some(bytes(1)), |writer| {
                writer.end_section(pi())
            })
        }
        StartCdata => writer.start_section(Frame::Cdata, b""),
        EndCdata => writer.end_section(Frame::Cdata),
        WriteCdata => {
            let opened = writer.start_section(Frame::Cdata, b"");
            write_body(writer, opened, Some(bytes(0)), |writer| {
                writer.end_section(Frame::Cdata)
            })
        }
        Text | WriteRaw => {
            writer.text(bytes(0), operation == WriteRaw);
            true
        }
        StartDocument => {
            return writer.start_document(optional(0).unwrap_or(b"1.0"), optional(1), optional(2));
        }
        EndDocument => writer.end_document(),
        WriteComment => {
            let opened = writer.start_section(Frame::Comment, b"");
            write_body(writer, opened, Some(bytes(0)), |writer| {
                writer.end_section(Frame::Comment)
            })
        }
        StartDtd => return dtd::start(writer, bytes(0), optional(1), optional(2)),
        EndDtd => dtd::end(writer),
        WriteDtd => {
            let opened = dtd::start(writer, bytes(0), optional(1), optional(2))?;
            write_body(writer, opened, optional(3), dtd::end)
        }
        StartDtdElement | StartDtdAttlist | StartDtdEntity => {
            dtd::start_declaration(writer, declaration(operation), bytes(0), enabled(1))
        }
        EndDtdElement | EndDtdAttlist | EndDtdEntity => {
            dtd::end_declaration(writer, declaration(operation))
        }
        WriteDtdElement | WriteDtdAttlist => {
            let kind = declaration(operation);
            let opened = dtd::start_declaration(writer, kind, bytes(0), false);
            write_body(writer, opened, Some(bytes(1)), |writer| {
                dtd::end_declaration(writer, kind)
            })
        }
        WriteDtdEntity => dtd::write_entity(
            writer,
            bytes(0),
            bytes(1),
            enabled(2),
            optional(3),
            optional(4),
            optional(5),
        ),
        OutputMemory | Flush => false,
    };
    Ok(result)
}

fn write_body(
    writer: &mut Writer,
    opened: bool,
    content: Option<&[u8]>,
    end: impl FnOnce(&mut Writer) -> bool,
) -> bool {
    if !opened {
        return false;
    }
    if let Some(content) = content {
        writer.text(content, false);
    }
    end(writer)
}

fn declaration(operation: Operation) -> Declaration {
    use Operation::*;
    match operation {
        StartDtdElement | EndDtdElement | WriteDtdElement => Declaration::Element,
        StartDtdAttlist | EndDtdAttlist | WriteDtdAttlist => Declaration::Attlist,
        _ => Declaration::Entity,
    }
}
