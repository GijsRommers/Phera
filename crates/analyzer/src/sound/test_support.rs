//! Compiled units written by hand for the tests of this module. The
//! analyzer does not depend on the compiler, so a test states the
//! instructions it means instead of the PHP that would produce them.

use std::path::PathBuf;
use std::sync::Arc;

use phera_bytecode::{
    ClassDeclaration, ClassKind, CompiledUnit, Constant, Function, FunctionId, Instruction,
    MethodDeclaration, NameId, Symbol, SymbolId, Visibility,
};

use super::{CompiledWorldUnit, DiscoveredFile};

/// A function body with room for the registers the tests use.
pub(crate) fn function(instructions: Vec<Instruction>) -> Function {
    Function {
        lines: vec![0; instructions.len()],
        instructions: instructions.into(),
        registers: 16,
        ..Function::default()
    }
}

pub(crate) fn named(name: &str, function: Function) -> Function {
    Function {
        name: name.to_owned(),
        ..function
    }
}

/// A file whose top-level code is `main`. As in every compiled file, the
/// top-level code has dynamic scope.
pub(crate) fn unit_of(main: Vec<Instruction>) -> CompiledUnit {
    CompiledUnit {
        main: Function {
            name: "{main}".to_owned(),
            dynamic_scope: true,
            ..function(main)
        },
        ..CompiledUnit::default()
    }
}

pub(crate) fn text(value: &str) -> Constant {
    Constant::String(value.as_bytes().to_vec())
}

pub(crate) fn symbol(name: &str, fallback: Option<&str>) -> Symbol {
    Symbol {
        name: name.to_owned(),
        fallback: fallback.map(str::to_owned),
        written: name.to_owned(),
    }
}

/// `class Name extends Parent {}` with nothing in it.
pub(crate) fn class(name: &str, parent: Option<SymbolId>) -> ClassDeclaration {
    ClassDeclaration {
        metadata: Default::default(),
        line: 1,
        name: name.to_owned(),
        parent,
        interfaces: Vec::new(),
        kind: ClassKind::Class,
        is_abstract: false,
        is_final: false,
        is_readonly: false,
        allows_dynamic_properties: false,
        constants: Vec::new(),
        properties: Vec::new(),
        methods: Vec::new(),
        traits: Vec::new(),
        enum_backing: None,
        enum_cases: Vec::new(),
    }
}

/// A public instance method: `unit.names[name]` compiled as
/// `unit.functions[function]`.
pub(crate) fn method(name: u32, function: u32) -> MethodDeclaration {
    MethodDeclaration {
        name: NameId(name),
        function: FunctionId(function),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
    }
}

/// A unit as a build holds it, at `/project/<relative_path>`.
pub(crate) fn world_unit(relative_path: &str, unit: CompiledUnit) -> CompiledWorldUnit {
    CompiledWorldUnit {
        file: DiscoveredFile {
            path: PathBuf::from("/project").join(relative_path),
            relative_path: relative_path.to_owned(),
            always_loaded: false,
            is_entry: false,
        },
        source_len: 0,
        source_hash: [0; 16],
        unit: Arc::new(unit),
    }
}
