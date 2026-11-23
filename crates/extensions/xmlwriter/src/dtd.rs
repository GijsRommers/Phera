//! Write document type declarations and their internal subset.
//!
//! ```php
//! $writer->startDtd('root');
//! $writer->writeDtdElement('root', 'EMPTY');
//! $writer->endDtd();
//! ```

use crate::writer::{Frame, Writer};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Declaration {
    Element,
    Attlist,
    Entity,
}

const IDENTIFIER_REQUIRED: &str = "xmlTextWriterStartDTD : system identifier needed!";
const DOCTYPE_POSITION: &str = "xmlTextWriterStartDTD : DTD allowed only in prolog!";

pub fn start(
    writer: &mut Writer,
    name: &[u8],
    public: Option<&[u8]>,
    system: Option<&[u8]>,
) -> Result<bool, &'static str> {
    if name.is_empty() {
        return Ok(false);
    }
    if !writer.frames.is_empty() {
        return Err(DOCTYPE_POSITION);
    }
    writer.output.extend_from_slice(b"<!DOCTYPE ");
    writer.output.extend_from_slice(name);
    writer.frames.push(Frame::Doctype { subset: false });
    if public.is_some() && system.is_none() {
        return Err(IDENTIFIER_REQUIRED);
    }
    external_ids(&mut writer.output, public, system, None);
    Ok(true)
}

pub fn end(writer: &mut Writer) -> bool {
    let Some(Frame::Doctype { .. }) = writer.frames.last() else {
        return false;
    };
    if let Some(Frame::Doctype { subset: true }) = writer.frames.pop() {
        writer.output.push(b']');
    }
    writer.output.push(b'>');
    writer.newline();
    true
}

pub fn start_declaration(
    writer: &mut Writer,
    declaration: Declaration,
    name: &[u8],
    parameter: bool,
) -> bool {
    if name.is_empty() || !matches!(writer.frames.last(), Some(Frame::Doctype { .. }) | None) {
        return false;
    }
    let in_doctype = !writer.frames.is_empty();
    if start_subset(writer) {
        writer.newline();
    }
    if in_doctype {
        writer.padding_at(1);
    }
    let keyword: &[u8] = match declaration {
        Declaration::Element => b"<!ELEMENT ",
        Declaration::Attlist => b"<!ATTLIST ",
        Declaration::Entity if parameter => b"<!ENTITY % ",
        Declaration::Entity => b"<!ENTITY ",
    };
    writer.output.extend_from_slice(keyword);
    writer.output.extend_from_slice(name);
    writer.frames.push(Frame::Declaration {
        kind: declaration,
        started: false,
    });
    true
}

pub fn end_declaration(writer: &mut Writer, declaration: Declaration) -> bool {
    let Some(Frame::Declaration { kind, started }) = writer.frames.last() else {
        return false;
    };
    if *kind != declaration {
        return false;
    }
    if *started && declaration == Declaration::Entity {
        writer.output.push(b'"');
    }
    writer.frames.pop();
    writer.output.push(b'>');
    writer.newline();
    true
}

pub fn text_prefix(writer: &mut Writer) {
    if let Some(Frame::Declaration { kind, started }) = writer.frames.last_mut() {
        if !*started {
            *started = true;
            writer.output.push(b' ');
            if *kind == Declaration::Entity {
                writer.output.push(b'"');
            }
        }
    } else {
        start_subset(writer);
    }
}

fn start_subset(writer: &mut Writer) -> bool {
    if let Some(Frame::Doctype { subset }) = writer.frames.last_mut()
        && !*subset
    {
        *subset = true;
        writer.output.extend_from_slice(b" [");
        return true;
    }
    false
}

pub fn write_entity(
    writer: &mut Writer,
    name: &[u8],
    value: &[u8],
    parameter: bool,
    public: Option<&[u8]>,
    system: Option<&[u8]>,
    notation: Option<&[u8]>,
) -> bool {
    if public.is_some() && system.is_none() || notation.is_some() && (parameter || system.is_none())
    {
        return false;
    }
    if !start_declaration(writer, Declaration::Entity, name, parameter) {
        return false;
    }
    if system.is_some() {
        external_ids(&mut writer.output, public, system, notation);
    } else {
        writer.text(value, true);
    }
    end_declaration(writer, Declaration::Entity)
}

fn external_ids(
    output: &mut Vec<u8>,
    public: Option<&[u8]>,
    system: Option<&[u8]>,
    notation: Option<&[u8]>,
) {
    if let Some(public) = public {
        output.extend_from_slice(b" PUBLIC \"");
        output.extend_from_slice(public);
        output.push(b'"');
    }
    if let Some(system) = system {
        output.extend_from_slice(if public.is_some() {
            b" \""
        } else {
            b" SYSTEM \""
        });
        output.extend_from_slice(system);
        output.push(b'"');
    }
    if let Some(notation) = notation {
        output.extend_from_slice(b" NDATA ");
        output.extend_from_slice(notation);
    }
}
