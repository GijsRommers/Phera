//! Signatures shared by XMLWriter's class methods and procedural aliases.
//!
//! ```php
//! $writer = xmlwriter_open_memory();
//! xmlwriter_write_element($writer, 'root', 'value');
//! echo $writer->outputMemory();
//! ```

use phera_extension_api::{ConstantValue, Param};

use crate::operations::Operation;
use crate::registration::{Binding, boolean, string, with_writer};

const SET_INDENT_ARGS: &[Param] = &[boolean("enable")];
const SET_INDENT_STRING_ARGS: &[Param] = &[string("indentation")];
const START_COMMENT_ARGS: &[Param] = &[];
const START_ATTRIBUTE_ARGS: &[Param] = &[string("name")];
const WRITE_ATTRIBUTE_ARGS: &[Param] = &[string("name"), string("value")];
const START_ATTRIBUTE_NS_ARGS: &[Param] = &[
    string("prefix").nullable(),
    string("name"),
    string("namespace").nullable(),
];
const WRITE_ATTRIBUTE_NS_ARGS: &[Param] = &[
    string("prefix").nullable(),
    string("name"),
    string("namespace").nullable(),
    string("value"),
];
const WRITE_ELEMENT_ARGS: &[Param] = &[
    string("name"),
    string("content")
        .nullable()
        .with_default(ConstantValue::Null),
];
const WRITE_ELEMENT_NS_ARGS: &[Param] = &[
    string("prefix").nullable(),
    string("name"),
    string("namespace").nullable(),
    string("content")
        .nullable()
        .with_default(ConstantValue::Null),
];
const START_PI_ARGS: &[Param] = &[string("target")];
const WRITE_PI_ARGS: &[Param] = &[string("target"), string("content")];
const WRITE_CDATA_ARGS: &[Param] = &[string("content")];
const START_DOCUMENT_ARGS: &[Param] = &[
    string("version")
        .nullable()
        .with_default(ConstantValue::String("1.0")),
    string("encoding")
        .nullable()
        .with_default(ConstantValue::Null),
    string("standalone")
        .nullable()
        .with_default(ConstantValue::Null),
];

pub const BINDINGS: &[Binding] = &[
    Binding::new::<{ Operation::Indent as usize }, 2>(
        Operation::Indent,
        "setIndent",
        "xmlwriter_set_indent",
        SET_INDENT_ARGS,
        const { &with_writer(SET_INDENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::IndentString as usize }, 2>(
        Operation::IndentString,
        "setIndentString",
        "xmlwriter_set_indent_string",
        SET_INDENT_STRING_ARGS,
        const { &with_writer(SET_INDENT_STRING_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartComment as usize }, 1>(
        Operation::StartComment,
        "startComment",
        "xmlwriter_start_comment",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndComment as usize }, 1>(
        Operation::EndComment,
        "endComment",
        "xmlwriter_end_comment",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartAttribute as usize }, 2>(
        Operation::StartAttribute,
        "startAttribute",
        "xmlwriter_start_attribute",
        START_ATTRIBUTE_ARGS,
        const { &with_writer(START_ATTRIBUTE_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndAttribute as usize }, 1>(
        Operation::EndAttribute,
        "endAttribute",
        "xmlwriter_end_attribute",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteAttribute as usize }, 3>(
        Operation::WriteAttribute,
        "writeAttribute",
        "xmlwriter_write_attribute",
        WRITE_ATTRIBUTE_ARGS,
        const { &with_writer(WRITE_ATTRIBUTE_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartAttributeNs as usize }, 4>(
        Operation::StartAttributeNs,
        "startAttributeNs",
        "xmlwriter_start_attribute_ns",
        START_ATTRIBUTE_NS_ARGS,
        const { &with_writer(START_ATTRIBUTE_NS_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteAttributeNs as usize }, 5>(
        Operation::WriteAttributeNs,
        "writeAttributeNs",
        "xmlwriter_write_attribute_ns",
        WRITE_ATTRIBUTE_NS_ARGS,
        const { &with_writer(WRITE_ATTRIBUTE_NS_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartElement as usize }, 2>(
        Operation::StartElement,
        "startElement",
        "xmlwriter_start_element",
        START_ATTRIBUTE_ARGS,
        const { &with_writer(START_ATTRIBUTE_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndElement as usize }, 1>(
        Operation::EndElement,
        "endElement",
        "xmlwriter_end_element",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::FullEndElement as usize }, 1>(
        Operation::FullEndElement,
        "fullEndElement",
        "xmlwriter_full_end_element",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartElementNs as usize }, 4>(
        Operation::StartElementNs,
        "startElementNs",
        "xmlwriter_start_element_ns",
        START_ATTRIBUTE_NS_ARGS,
        const { &with_writer(START_ATTRIBUTE_NS_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteElement as usize }, 3>(
        Operation::WriteElement,
        "writeElement",
        "xmlwriter_write_element",
        WRITE_ELEMENT_ARGS,
        const { &with_writer(WRITE_ELEMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteElementNs as usize }, 5>(
        Operation::WriteElementNs,
        "writeElementNs",
        "xmlwriter_write_element_ns",
        WRITE_ELEMENT_NS_ARGS,
        const { &with_writer(WRITE_ELEMENT_NS_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartPi as usize }, 2>(
        Operation::StartPi,
        "startPi",
        "xmlwriter_start_pi",
        START_PI_ARGS,
        const { &with_writer(START_PI_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndPi as usize }, 1>(
        Operation::EndPi,
        "endPi",
        "xmlwriter_end_pi",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WritePi as usize }, 3>(
        Operation::WritePi,
        "writePi",
        "xmlwriter_write_pi",
        WRITE_PI_ARGS,
        const { &with_writer(WRITE_PI_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartCdata as usize }, 1>(
        Operation::StartCdata,
        "startCdata",
        "xmlwriter_start_cdata",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndCdata as usize }, 1>(
        Operation::EndCdata,
        "endCdata",
        "xmlwriter_end_cdata",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteCdata as usize }, 2>(
        Operation::WriteCdata,
        "writeCdata",
        "xmlwriter_write_cdata",
        WRITE_CDATA_ARGS,
        const { &with_writer(WRITE_CDATA_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::Text as usize }, 2>(
        Operation::Text,
        "text",
        "xmlwriter_text",
        WRITE_CDATA_ARGS,
        const { &with_writer(WRITE_CDATA_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteRaw as usize }, 2>(
        Operation::WriteRaw,
        "writeRaw",
        "xmlwriter_write_raw",
        WRITE_CDATA_ARGS,
        const { &with_writer(WRITE_CDATA_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartDocument as usize }, 4>(
        Operation::StartDocument,
        "startDocument",
        "xmlwriter_start_document",
        START_DOCUMENT_ARGS,
        const { &with_writer(START_DOCUMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndDocument as usize }, 1>(
        Operation::EndDocument,
        "endDocument",
        "xmlwriter_end_document",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteComment as usize }, 2>(
        Operation::WriteComment,
        "writeComment",
        "xmlwriter_write_comment",
        WRITE_CDATA_ARGS,
        const { &with_writer(WRITE_CDATA_ARGS) },
        "bool",
    ),
];
