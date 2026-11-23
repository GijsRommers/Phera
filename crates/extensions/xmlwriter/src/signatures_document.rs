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

const START_COMMENT_ARGS: &[Param] = &[];
const START_ATTRIBUTE_ARGS: &[Param] = &[string("name")];
const START_DTD_ARGS: &[Param] = &[
    string("qualifiedName"),
    string("publicId")
        .nullable()
        .with_default(ConstantValue::Null),
    string("systemId")
        .nullable()
        .with_default(ConstantValue::Null),
];
const WRITE_DTD_ARGS: &[Param] = &[
    string("name"),
    string("publicId")
        .nullable()
        .with_default(ConstantValue::Null),
    string("systemId")
        .nullable()
        .with_default(ConstantValue::Null),
    string("content")
        .nullable()
        .with_default(ConstantValue::Null),
];
const START_DTD_ELEMENT_ARGS: &[Param] = &[string("qualifiedName")];
const WRITE_DTD_ELEMENT_ARGS: &[Param] = &[string("name"), string("content")];
const START_DTD_ENTITY_ARGS: &[Param] = &[string("name"), boolean("isParam")];
const WRITE_DTD_ENTITY_ARGS: &[Param] = &[
    string("name"),
    string("content"),
    boolean("isParam").with_default(ConstantValue::Bool(false)),
    string("publicId")
        .nullable()
        .with_default(ConstantValue::Null),
    string("systemId")
        .nullable()
        .with_default(ConstantValue::Null),
    string("notationData")
        .nullable()
        .with_default(ConstantValue::Null),
];
const OUTPUT_MEMORY_ARGS: &[Param] = &[boolean("flush").with_default(ConstantValue::Bool(true))];
const FLUSH_ARGS: &[Param] = &[boolean("empty").with_default(ConstantValue::Bool(true))];

pub const BINDINGS: &[Binding] = &[
    Binding::new::<{ Operation::StartDtd as usize }, 4>(
        Operation::StartDtd,
        "startDtd",
        "xmlwriter_start_dtd",
        START_DTD_ARGS,
        const { &with_writer(START_DTD_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndDtd as usize }, 1>(
        Operation::EndDtd,
        "endDtd",
        "xmlwriter_end_dtd",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteDtd as usize }, 5>(
        Operation::WriteDtd,
        "writeDtd",
        "xmlwriter_write_dtd",
        WRITE_DTD_ARGS,
        const { &with_writer(WRITE_DTD_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartDtdElement as usize }, 2>(
        Operation::StartDtdElement,
        "startDtdElement",
        "xmlwriter_start_dtd_element",
        START_DTD_ELEMENT_ARGS,
        const { &with_writer(START_DTD_ELEMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndDtdElement as usize }, 1>(
        Operation::EndDtdElement,
        "endDtdElement",
        "xmlwriter_end_dtd_element",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteDtdElement as usize }, 3>(
        Operation::WriteDtdElement,
        "writeDtdElement",
        "xmlwriter_write_dtd_element",
        WRITE_DTD_ELEMENT_ARGS,
        const { &with_writer(WRITE_DTD_ELEMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartDtdAttlist as usize }, 2>(
        Operation::StartDtdAttlist,
        "startDtdAttlist",
        "xmlwriter_start_dtd_attlist",
        START_ATTRIBUTE_ARGS,
        const { &with_writer(START_ATTRIBUTE_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndDtdAttlist as usize }, 1>(
        Operation::EndDtdAttlist,
        "endDtdAttlist",
        "xmlwriter_end_dtd_attlist",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteDtdAttlist as usize }, 3>(
        Operation::WriteDtdAttlist,
        "writeDtdAttlist",
        "xmlwriter_write_dtd_attlist",
        WRITE_DTD_ELEMENT_ARGS,
        const { &with_writer(WRITE_DTD_ELEMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::StartDtdEntity as usize }, 3>(
        Operation::StartDtdEntity,
        "startDtdEntity",
        "xmlwriter_start_dtd_entity",
        START_DTD_ENTITY_ARGS,
        const { &with_writer(START_DTD_ENTITY_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::EndDtdEntity as usize }, 1>(
        Operation::EndDtdEntity,
        "endDtdEntity",
        "xmlwriter_end_dtd_entity",
        START_COMMENT_ARGS,
        const { &with_writer(START_COMMENT_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::WriteDtdEntity as usize }, 7>(
        Operation::WriteDtdEntity,
        "writeDtdEntity",
        "xmlwriter_write_dtd_entity",
        WRITE_DTD_ENTITY_ARGS,
        const { &with_writer(WRITE_DTD_ENTITY_ARGS) },
        "bool",
    ),
    Binding::new::<{ Operation::OutputMemory as usize }, 2>(
        Operation::OutputMemory,
        "outputMemory",
        "xmlwriter_output_memory",
        OUTPUT_MEMORY_ARGS,
        const { &with_writer(OUTPUT_MEMORY_ARGS) },
        "string",
    ),
    Binding::new::<{ Operation::Flush as usize }, 2>(
        Operation::Flush,
        "flush",
        "xmlwriter_flush",
        FLUSH_ARGS,
        const { &with_writer(FLUSH_ARGS) },
        "string|int",
    ),
];
