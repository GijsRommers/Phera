//! Converts compiler paths and source names into compact bytecode tables.
//!
//! Input IR has final register numbers and IR-position jump targets. Output
//! bytecode owns interned tables and has targets remapped after expansion.
//! For example, Read(local) becomes Move, while Read(indexed) becomes Fetch
//! with a newly appended path-table entry.

use phera_bytecode::{
    CompiledUnit, ConstId, Constant, Function, FunctionId, Instruction, NameId, Parameter, Reg,
    Symbol, SymbolId, Type, TypeId, TypeMask,
};

use crate::ir;

/// Final table builder. No compiler-only paths escape in instructions.
#[derive(Default)]
pub(crate) struct Tables {
    unit: CompiledUnit,
    /// The file's top-level functions, which calls reach with `CallDirect`.
    callees: crate::passes::direct_calls::DirectCallees,
}

impl Tables {
    /// Intern names in first-use order to keep table indices deterministic.
    pub(crate) fn name(&mut self, name: &str) -> NameId {
        let index = self
            .unit
            .names
            .iter()
            .position(|entry| entry == name)
            .unwrap_or_else(|| {
                self.unit.names.push(name.to_owned());
                self.unit.names.len() - 1
            });
        NameId(index as u32)
    }

    /// Recognize literal values without evaluating arbitrary PHP expressions.
    pub(crate) fn constant(&mut self, constant: &ir::Constant) -> ConstId {
        let value = match constant {
            ir::Constant::EmptyArray => Constant::Array(vec![]),
            ir::Constant::Null => Constant::Null,
            ir::Constant::Bool(value) => Constant::Bool(*value),
            ir::Constant::Int(value) => Constant::Int(*value),
            ir::Constant::Float(value) => Constant::Float(*value),
            ir::Constant::Bytes(value) => Constant::String(value.clone()),
        };
        let index = self
            .unit
            .constants
            .iter()
            .position(|entry| entry == &value)
            .unwrap_or_else(|| {
                self.unit.constants.push(value);
                self.unit.constants.len() - 1
            });
        ConstId(index as u32)
    }

    /// Intern a resolved symbol, folding function names but preserving constant spelling.
    pub(crate) fn symbol(
        &mut self,
        name: &str,
        fallback: Option<String>,
        kind: crate::policies::NameKind,
    ) -> SymbolId {
        let symbol = Symbol {
            name: if kind == crate::policies::NameKind::Function {
                name.to_ascii_lowercase()
            } else {
                name.to_owned()
            },
            fallback: fallback.map(|name| {
                if kind == crate::policies::NameKind::Function {
                    name.to_ascii_lowercase()
                } else {
                    name
                }
            }),
            written: name.to_owned(),
        };
        let index = self
            .unit
            .symbols
            .iter()
            .position(|entry| entry == &symbol)
            .unwrap_or_else(|| {
                self.unit.symbols.push(symbol);
                self.unit.symbols.len() - 1
            });
        SymbolId(index as u32)
    }

    /// Translate a written type to its mask and class-name table entries.
    pub(crate) fn ty(&mut self, display: &str) -> TypeId {
        let mut mask = TypeMask::default();
        let mut classes = Vec::new();
        if display.starts_with('?') {
            mask = TypeMask::NULL;
        }
        for name in display.trim_start_matches('?').split(['|', '&']) {
            let builtin = match name {
                "null" => TypeMask::NULL,
                "false" => TypeMask::FALSE,
                "true" => TypeMask::TRUE,
                "bool" => TypeMask::BOOL,
                "int" => TypeMask::INT,
                "float" => TypeMask::FLOAT,
                "string" => TypeMask::STRING,
                "array" => TypeMask::ARRAY,
                "object" => TypeMask::OBJECT,
                "callable" => TypeMask::CALLABLE,
                "iterable" => TypeMask::ITERABLE,
                "void" => TypeMask::VOID,
                "never" => TypeMask::NEVER,
                "mixed" => TypeMask::MIXED,
                _ => {
                    classes.push(self.name(&name.to_ascii_lowercase()));
                    TypeMask::default()
                }
            };
            mask = mask.union(builtin);
        }
        let index = self.unit.types.len();
        self.unit.types.push(Type {
            mask,
            classes,
            intersection: display.contains('&'),
            display: display.into(),
        });
        TypeId(index as u32)
    }

    /// Emit a function, translate its jump offsets, then apply operand optimizations.
    fn function(&mut self, source: ir::Function) -> Function {
        let mut function = Function {
            name: source.name,
            definition_line: source.definition_line as u32,
            registers: source.registers as u16 + 1,
            locals: source.locals.iter().map(|name| self.name(name)).collect(),
            parameters: source
                .parameters
                .iter()
                .map(|parameter| Parameter {
                    default_display: parameter.default_display.clone(),
                    name: self.name(&parameter.name),
                    ty: parameter.declaration_type.as_deref().map(|ty| self.ty(ty)),
                    by_reference: parameter.by_reference,
                    variadic: parameter.variadic,
                    has_default: parameter.has_default,
                })
                .collect(),
            captures: source.captures as u16,
            argument_layout: if source
                .parameters
                .last()
                .is_some_and(|parameter| parameter.variadic)
            {
                phera_bytecode::ArgumentLayout::Variadic
            } else {
                phera_bytecode::ArgumentLayout::Fixed
            },
            returns_reference: source.returns_reference,
            is_generator: source.is_generator,
            return_type: source.return_type.as_deref().map(|ty| self.ty(ty)),
            dynamic_scope: source.dynamic_scope,
            strict_types: source.strict_types,
            ..Function::default()
        };
        let scratch = Reg(source.registers as u16);
        let mut offsets = Vec::new();
        for (index, instruction) in source.instructions.into_iter().enumerate() {
            offsets.push(function.instructions.len() as u32);
            self.instruction(&mut function, instruction, &source.constants, scratch);
            let line = source.lines.get(index).copied().unwrap_or(1) as u32;
            function.lines.resize(function.instructions.len(), line);
        }
        offsets.push(function.instructions.len() as u32);
        // Prologue targets are already final. Source jumps still name IR positions.
        for instruction in &mut function.instructions {
            let target = match instruction {
                Instruction::FinallyCall { target, .. }
                | Instruction::ReceiveDefault { passed: target, .. }
                | Instruction::BindStatic {
                    initialized: target,
                    ..
                }
                | Instruction::Jump { target }
                | Instruction::JumpIf { target, .. }
                | Instruction::JumpIfNot { target, .. }
                | Instruction::JumpIfNull { target, .. }
                | Instruction::IterNext { end: target, .. }
                | Instruction::IterNextKeyed { end: target, .. } => Some(target),
                _ => None,
            };
            if let Some(target) = target {
                target.0 = offsets.get(target.index()).copied().unwrap_or(target.0);
            }
        }
        function.handlers = source
            .handlers
            .into_iter()
            .map(|mut handler| {
                handler.start.0 = offsets[handler.start.index()];
                handler.end.0 = offsets[handler.end.index()];
                handler.catch = handler
                    .catch
                    .map(|target| phera_bytecode::Target(offsets[target.index()]));
                handler.finally = handler
                    .finally
                    .map(|target| phera_bytecode::Target(offsets[target.index()]));
                handler
            })
            .collect();
        function.observes_arguments =
            function
                .instructions
                .iter()
                .any(|instruction| match instruction {
                    Instruction::InitDynamicCall { .. } => true,
                    Instruction::InitCall { function } => self
                        .unit
                        .symbols
                        .get(function.index())
                        .is_some_and(|symbol| {
                            [Some(symbol.name.as_str()), symbol.fallback.as_deref()]
                                .into_iter()
                                .flatten()
                                .any(|name| {
                                    matches!(
                                        name.rsplit('\\').next(),
                                        Some(
                                            "func_get_args"
                                                | "func_get_arg"
                                                | "func_num_args"
                                                | "call_user_func"
                                                | "call_user_func_array"
                                        )
                                    )
                                })
                        }),
                    _ => false,
                });
        let areas =
            crate::passes::direct_calls::rewrite(&mut function, &self.unit.symbols, &self.callees);
        crate::passes::optimize::optimize(&mut function);
        if let Some(areas) = areas
            && crate::passes::direct_calls::write_arguments_in_place(&mut function, areas)
        {
            // Removes the moves that became `Nop`.
            crate::passes::optimize::optimize(&mut function);
        }
        function
    }
}

/// The functions a file declares at its top level, which exist before any of
/// its code runs, with what a direct call to them needs to know.
fn direct_callees(source: &ir::CompiledUnit) -> crate::passes::direct_calls::DirectCallees {
    source
        .hoisted
        .iter()
        .filter_map(|&index| {
            let function = source.functions.get(index)?;
            let callee = crate::passes::direct_calls::DirectCallee {
                function: FunctionId(index as u32),
                parameters: function.parameters.len(),
                needs_general_call: function
                    .parameters
                    .iter()
                    .any(|parameter| parameter.variadic || parameter.by_reference),
            };
            Some((function.name.to_ascii_lowercase(), callee))
        })
        .collect()
}

/// Intern all file tables once compilation has completed.
pub(crate) fn lower(source: ir::CompiledUnit) -> CompiledUnit {
    let mut tables = Tables {
        callees: direct_callees(&source),
        ..Tables::default()
    };
    tables.unit.file = source.file;
    tables.unit.reflection_initializers = source.reflection_initializers;
    tables.unit.reflection_source = source.reflection_source;
    tables.unit.constant_metadata = source.constant_metadata;
    tables.unit.has_link_attributes = source.has_link_attributes;
    tables.unit.declaration_warnings = source.declaration_warnings;
    tables.unit.main = tables.function(source.main);
    tables.unit.functions = source
        .functions
        .into_iter()
        .map(|mut function| {
            tables
                .unit
                .function_metadata
                .push(std::mem::take(&mut function.metadata));
            tables.function(function)
        })
        .collect();
    tables.unit.hoisted = source
        .hoisted
        .into_iter()
        .map(|index| FunctionId(index as u32))
        .collect();
    tables.unit.classes = source
        .classes
        .into_iter()
        .map(|declaration| phera_bytecode::ClassDeclaration {
            metadata: declaration.metadata,
            line: declaration.line,
            name: declaration.name,
            parent: declaration
                .parent
                .map(|name| tables.symbol(&name, None, crate::policies::NameKind::Class)),
            interfaces: declaration
                .interfaces
                .into_iter()
                .map(|name| tables.symbol(&name, None, crate::policies::NameKind::Class))
                .collect(),
            kind: declaration.kind,
            is_abstract: declaration.is_abstract,
            is_final: declaration.is_final,
            is_readonly: declaration.is_readonly,
            allows_dynamic_properties: declaration.allows_dynamic_properties,
            traits: declaration.traits,
            enum_backing: declaration.enum_backing,
            enum_cases: declaration
                .enum_cases
                .into_iter()
                .map(|(name, backing, metadata)| phera_bytecode::EnumCase {
                    metadata,
                    name: tables.name(&name),
                    backing: backing.map(|index| FunctionId(index as u32)),
                })
                .collect(),
            constants: declaration
                .constants
                .into_iter()
                .map(|constant| phera_bytecode::ClassConstant {
                    metadata: constant.metadata,
                    name: tables.name(&constant.name),
                    visibility: constant.visibility,
                    is_final: constant.is_final,
                    initializer: FunctionId(constant.initializer as u32),
                    ty: constant.ty.map(|name| tables.ty(&name)),
                })
                .collect(),
            properties: declaration
                .properties
                .into_iter()
                .map(|property| phera_bytecode::PropertyDeclaration {
                    metadata: property.metadata,
                    promoted: property.promoted,
                    set_visibility: property.set_visibility,
                    is_final: property.is_final,
                    name: tables.name(&property.name),
                    visibility: property.visibility,
                    is_static: property.is_static,
                    is_readonly: property.is_readonly,
                    ty: property.ty.map(|name| tables.ty(&name)),
                    default: property.default.map(|index| {
                        phera_bytecode::Initializer::Function(FunctionId(index as u32))
                    }),
                })
                .collect(),
            methods: declaration
                .methods
                .into_iter()
                .map(|method| phera_bytecode::MethodDeclaration {
                    name: tables.name(&method.name),
                    function: FunctionId(method.function as u32),
                    visibility: method.visibility,
                    is_static: method.is_static,
                    is_abstract: method.is_abstract,
                    is_final: method.is_final,
                })
                .collect(),
        })
        .collect();
    tables.unit.hoisted_classes = source
        .hoisted_classes
        .into_iter()
        .map(|index| phera_bytecode::ClassId(index as u32))
        .collect();
    tables.unit
}
