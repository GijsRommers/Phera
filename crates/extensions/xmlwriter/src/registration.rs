//! Publish the XMLWriter class, its factories, and procedural functions.
//!
//! ```php
//! extension_loaded('xmlwriter'); // true
//! $writer = new XMLWriter();
//! $writer->openMemory();
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, FunctionEntry, MethodEntry, NativeClone, Param, ParamType, Registry,
};

use crate::factory;
use crate::operations::{self, Operation};
use crate::{signatures_document, signatures_markup};

const WRITER: Param = Param::new("writer", ParamType::Class(factory::CLASS));
const URI: &[Param] = &[Param::new("uri", ParamType::String)];

#[derive(Clone, Copy)]
pub struct Binding {
    pub operation: Operation,
    pub method: MethodEntry,
    pub function: FunctionEntry,
    result: &'static str,
}

impl Binding {
    pub const fn new<const INDEX: usize, const LEN: usize>(
        operation: Operation,
        method: &'static str,
        function: &'static str,
        params: &'static [Param],
        procedural: &'static [Param; LEN],
        result: &'static str,
    ) -> Self {
        Self {
            operation,
            method: MethodEntry::public_async(method, params, operations::method::<INDEX>)
                .tentative_return(result),
            function: FunctionEntry::asynchronous(
                function,
                procedural,
                operations::function::<INDEX>,
            ),
            result,
        }
    }
}

const MARKUP: &[Binding] = signatures_markup::BINDINGS;
const DOCUMENT: &[Binding] = signatures_document::BINDINGS;

pub const BINDINGS: [Binding; MARKUP.len() + DOCUMENT.len()] = {
    let mut entries = [MARKUP[0]; MARKUP.len() + DOCUMENT.len()];
    let mut index = 0;
    while index < MARKUP.len() {
        entries[index] = MARKUP[index];
        index += 1;
    }
    let mut document = 0;
    while document < DOCUMENT.len() {
        entries[index] = DOCUMENT[document];
        document += 1;
        index += 1;
    }
    entries
};

const FACTORIES: [MethodEntry; 5] = [
    MethodEntry::public_async("openUri", URI, factory::open_uri).tentative_return("bool"),
    MethodEntry::public_async("openMemory", &[], factory::open_memory).tentative_return("bool"),
    MethodEntry::public_async("toUri", URI, factory::to_uri)
        .static_()
        .returning("static"),
    MethodEntry::public_async("toMemory", &[], factory::to_memory)
        .static_()
        .returning("static"),
    MethodEntry::public_async(
        "toStream",
        &[Param::new("stream", ParamType::Mixed)],
        factory::to_stream,
    )
    .static_()
    .returning("static"),
];

static METHODS: [MethodEntry; BINDINGS.len() + FACTORIES.len()] = {
    let mut entries = [FACTORIES[0]; BINDINGS.len() + FACTORIES.len()];
    let mut index = 0;
    while index < FACTORIES.len() {
        entries[index] = FACTORIES[index];
        index += 1;
    }
    let mut binding = 0;
    while binding < BINDINGS.len() {
        assert!(BINDINGS[binding].operation as usize == binding);
        entries[index] = BINDINGS[binding].method;
        binding += 1;
        index += 1;
    }
    entries
};

static HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: &METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(factory::CLASS)
    });
    registry.function(FunctionEntry::asynchronous(
        "xmlwriter_open_uri",
        URI,
        factory::procedural_open_uri,
    ));
    registry.function(FunctionEntry::asynchronous(
        "xmlwriter_open_memory",
        &[],
        factory::procedural_open_memory,
    ));
    registry.describe_function("xmlwriter_open_uri", "XMLWriter|false", None);
    registry.describe_function("xmlwriter_open_memory", "XMLWriter|false", None);
    for binding in BINDINGS {
        registry.function(binding.function);
        registry.describe_function(binding.function.name, binding.result, None);
    }
}

pub const fn string(name: &'static str) -> Param {
    Param::new(name, ParamType::String)
}

pub const fn boolean(name: &'static str) -> Param {
    Param::new(name, ParamType::Bool)
}

pub const fn with_writer<const LEN: usize>(params: &[Param]) -> [Param; LEN] {
    let mut result = [WRITER; LEN];
    let mut index = 0;
    while index < params.len() {
        result[index + 1] = params[index];
        index += 1;
    }
    result
}
