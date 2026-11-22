//! Compilation state and register allocation shared by AST lowering passes.
//!
//! AST nodes lower to compiler IR here; final interning belongs to `lower`.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use phera_ast as ast;
use phera_bytecode::Reg;
use phera_bytecode::reflection::{Imports, InitializerSource};
use phera_source::{CompileDiagnostic, LineTable};

use crate::CompileError;
use crate::ir::{Constant, Function, Instruction, Target};
use crate::policies::{DeclarationTiming, NameKind};

/// Unresolved exits for one nested loop or switch context.
pub(super) struct Loop {
    pub(super) start: usize,
    pub(super) cleanup: Option<crate::statements::exits::LoopCleanup>,
    pub(super) breaks: Vec<usize>,
    pub(super) continues: Vec<usize>,
}
/// Source retained only for comments and initializer expressions that reflection may read.
#[derive(Default)]
pub(crate) struct ReflectionInputs {
    pub(crate) initializers: RefCell<Vec<InitializerSource>>,
    pub(crate) has_doc_comments: Cell<bool>,
    pub(crate) has_link_attributes: Cell<bool>,
}

/// Mutable AST-to-IR state for one PHP function, sharing source locations.
pub(super) struct Compiler<'ast> {
    pub(super) file: &'ast str,
    /// Source bytes for quoted assertions, declaration metadata and deferred expressions.
    pub(super) source: &'ast [u8],
    pub(super) declaration_warnings: Vec<phera_bytecode::DeclarationWarning>,
    pub(super) compile_diagnostics: &'ast RefCell<Vec<CompileDiagnostic>>,
    pub(super) lines: &'ast LineTable,
    pub(super) function: Function,
    pub(super) functions: Vec<Function>,
    pub(super) hoisted: Vec<usize>,
    /// The lowercased names of the functions in `hoisted`, to find a
    /// redeclaration without searching every earlier one.
    pub(super) hoisted_names: std::collections::HashSet<String>,
    pub(super) constant_metadata: HashMap<String, phera_bytecode::reflection::DeclarationMetadata>,
    pub(super) classes: Vec<crate::ir::ClassDeclaration>,
    pub(super) hoisted_classes: Vec<usize>,
    pub(super) anonymous_classes: Rc<Cell<u64>>,
    pub(super) next: usize,
    pub(super) peak: usize,
    /// The highest `next` since the current expression statement began; see
    /// [`Compiler::clear_temporaries_after`].
    pub(super) reach: usize,
    /// The operand of the long-chain step being compiled; see
    /// [`crate::expressions::chains`].
    pub(super) compiled_operand: Option<crate::expressions::chains::CompiledOperand>,
    pub(super) locals: HashMap<String, Reg>,
    /// File-scope declaration order is independent of register allocation.
    file_symbols: Option<HashMap<String, usize>>,
    pub(super) loops: Vec<Loop>,
    /// Lexical cleanup scopes decide which finally blocks a structured exit crosses.
    pub(super) finalies: Vec<crate::statements::exceptions::FinallyScope>,
    pub(super) labels: Option<Box<crate::statements::gotos::Labels>>,
    pub(super) namespace: String,
    /// __CLASS__ is lexical and does not change when a closure is rebound.
    pub(super) class_scope: Option<String>,
    pub(super) trait_scope: Option<String>,
    /// The resolved `extends` of the class being compiled, when it is a class
    /// with a parent: the name `parent::` reads are compiled to in a method.
    pub(super) parent_scope: Option<String>,
    pub(super) hooked_property: Option<String>,
    pub(super) uses_hook_backing: bool,
    pub(super) imports: Option<Imports>,
    pub(super) reflection: &'ast ReflectionInputs,
    /// Temporaries cleared right after the call that consumed them (see
    /// `expressions::operand_release`), which the statement's end need not
    /// clear again until they are allocated anew.
    pub(super) released: HashSet<u16>,
    pub(super) native_functions: Option<&'ast crate::NativeFunctions<'ast>>,
}
impl<'ast> Compiler<'ast> {
    /// Start an independent function builder in the current namespace.
    pub(super) fn new(
        file: &'ast str,
        source: &'ast [u8],
        lines: &'ast LineTable,
        name: &str,
        namespace: &str,
        compile_diagnostics: &'ast RefCell<Vec<CompileDiagnostic>>,
        reflection: &'ast ReflectionInputs,
    ) -> Self {
        Self {
            file,
            source,
            declaration_warnings: Vec::new(),
            compile_diagnostics,
            lines,
            function: Function {
                name: name.into(),
                ..Function::default()
            },
            functions: vec![],
            hoisted: vec![],
            hoisted_names: std::collections::HashSet::new(),
            constant_metadata: HashMap::new(),
            classes: vec![],
            hoisted_classes: vec![],
            anonymous_classes: Rc::default(),
            next: 0,
            peak: 0,
            reach: 0,
            compiled_operand: None,
            locals: HashMap::new(),
            file_symbols: (name == "{main}").then(HashMap::new),
            loops: vec![],
            finalies: vec![],
            labels: None,
            namespace: namespace.into(),
            class_scope: None,
            trait_scope: None,
            parent_scope: None,
            hooked_property: None,
            uses_hook_backing: false,
            imports: None,
            reflection,
            released: HashSet::new(),
            native_functions: None,
        }
    }

    /// Attach the current file to a compiler diagnostic without changing PHP wording.
    pub(super) fn error(&self, message: impl Into<String>, line: usize) -> CompileError {
        CompileError {
            message: message.into(),
            file: self.file.into(),
            line,
            parse: false,
            scanner_diagnostics: Vec::new(),
        }
    }

    /// Reserve a temporary slot and reject programs exceeding the bytecode register limit.
    pub(super) fn allocate_register(&mut self) -> Result<Reg, CompileError> {
        if self.next >= u16::MAX as usize - 1 {
            return Err(self.error("Phera register limit exceeded", 1));
        }
        let index =
            u16::try_from(self.next).map_err(|_| self.error("Phera register limit exceeded", 1))?;
        self.next += 1;
        self.peak = self.peak.max(self.next);
        self.reach = self.reach.max(self.next);
        self.released.remove(&index);
        Ok(Reg(index))
    }

    /// Allocate each named local once; finalization moves these slots ahead of temporaries.
    pub(super) fn local(&mut self, name: &str) -> Result<Reg, CompileError> {
        if let Some(register) = self.locals.get(name) {
            return Ok(*register);
        }
        self.declare_file_symbol(name);
        // A new local must never reuse a slot mentioned by earlier temporary code.
        self.next = self.next.max(self.peak);
        let register = self.allocate_register()?;
        self.locals.insert(name.into(), register);
        self.function.locals.push(name.into());
        Ok(register)
    }

    /// PHP declares a named assignment target before its right-hand side,
    /// while Phera allocates the register when it emits the write.
    pub(super) fn declare_file_symbol(&mut self, name: &str) {
        if let Some(symbols) = &mut self.file_symbols
            && !symbols.contains_key(name)
        {
            symbols.insert(name.into(), symbols.len());
        }
    }

    /// Renumber locals before temporaries without changing instruction order.
    pub(super) fn finish(&mut self) -> Result<Function, CompileError> {
        self.finish_gotos()?;
        if let Some(symbols) = &self.file_symbols {
            self.function
                .locals
                .sort_unstable_by_key(|name| symbols.get(name).copied().unwrap_or(usize::MAX));
        }
        let mut mapping = vec![Reg(0); self.peak];
        let mut named = vec![false; self.peak];
        for (i, name) in self.function.locals.iter().enumerate() {
            if let Some(old) = self.locals.get(name) {
                if let Some(slot) = mapping.get_mut(old.0 as usize) {
                    *slot = Reg(i as u16)
                }
                if let Some(slot) = named.get_mut(old.0 as usize) {
                    *slot = true
                }
            }
        }
        let mut next = self.function.locals.len();
        for (i, is_named) in named.into_iter().enumerate() {
            if !is_named {
                if let Some(slot) = mapping.get_mut(i) {
                    *slot = Reg(next as u16)
                }
                next += 1;
            }
        }
        for instruction in &mut self.function.instructions {
            instruction.map_registers(|register| {
                mapping
                    .get(register.0 as usize)
                    .copied()
                    .unwrap_or(register)
            });
        }
        for handler in &mut self.function.handlers {
            handler.exception = mapping
                .get(handler.exception.index())
                .copied()
                .unwrap_or(handler.exception);
            for register in &mut handler.cleanup {
                *register = mapping.get(register.index()).copied().unwrap_or(*register);
            }
        }
        self.function.registers = self.peak;
        Ok(std::mem::take(&mut self.function))
    }

    /// Append an IR instruction and its source line together; return its patch position.
    pub(super) fn emit(&mut self, i: Instruction, line: usize) -> usize {
        let index = self.function.instructions.len();
        self.function.instructions.push(i);
        self.function.lines.push(line);
        index
    }

    /// Return the next IR instruction position for a control-flow target.
    pub(super) fn position(&self) -> usize {
        self.function.instructions.len()
    }

    /// Resolve a previously emitted control-flow edge to its destination.
    pub(super) fn patch(&mut self, i: usize, to: usize) {
        if let Some(
            Instruction::DefaultCheck { passed: t, .. }
            | Instruction::StaticCheck { initialized: t, .. }
            | Instruction::Jump(t)
            | Instruction::Branch { target: t, .. }
            | Instruction::BranchNull { target: t, .. }
            | Instruction::BranchNotNull { target: t, .. }
            | Instruction::GuardBuiltin { call: t, .. }
            | Instruction::BranchCompare { target: t, .. }
            | Instruction::BranchCompareInt { target: t, .. }
            | Instruction::BranchInstanceOf { target: t, .. }
            | Instruction::BranchDynamicInstanceOf { target: t, .. }
            | Instruction::IterNext { end: t, .. },
        ) = self.function.instructions.get_mut(i)
        {
            *t = Target(to)
        }
    }

    /// Record a literal and load it into a fresh register.
    pub(super) fn literal(&mut self, value: Constant, line: usize) -> Result<Reg, CompileError> {
        let dst = self.allocate_register()?;
        let constant = self.function.constants.len();
        self.function.constants.push(value);
        self.emit(
            Instruction::Load {
                destination: dst,
                constant,
            },
            line,
        );
        Ok(dst)
    }

    /// Resolve imports before namespace fallback, preserving PHP qualification rules.
    pub(super) fn resolve(&self, index: &ast::Name, kind: NameKind) -> (String, Option<String>) {
        if index.kind == ast::NameKind::FullyQualified {
            return (index.text.clone(), None);
        }
        if index.kind != ast::NameKind::Relative
            && let Some(imports) = &self.imports
        {
            let (first, rest) = index.text.split_once('\\').unwrap_or((&index.text, ""));
            if let Some(value) = imports.get(&(
                if rest.is_empty() {
                    kind
                } else {
                    NameKind::Class
                },
                first.to_ascii_lowercase(),
            )) {
                return (
                    if rest.is_empty() {
                        value.clone()
                    } else {
                        format!("{value}\\{rest}")
                    },
                    None,
                );
            }
        }
        let full = if self.namespace.is_empty() {
            index.text.clone()
        } else {
            format!("{}\\{}", self.namespace, index.text)
        };
        let fallback = (index.kind == ast::NameKind::Unqualified && !self.namespace.is_empty())
            .then(|| index.text.clone());
        (full, fallback)
    }

    /// Compile statements in source order with the enclosing declaration policy.
    pub(super) fn block(
        &mut self,
        statements: &[ast::Statement],
        timing: DeclarationTiming,
    ) -> Result<(), CompileError> {
        for statement in statements {
            self.statement(statement, timing)?
        }
        Ok(())
    }

    /// Patch break exits and continue edges for the innermost loop or switch.
    pub(super) fn loop_end(&mut self, continue_at: usize) {
        if let Some(loop_state) = self.loops.pop() {
            let end = self.position();
            for i in loop_state.breaks {
                self.patch(i, end)
            }
            for i in loop_state.continues {
                self.patch(i, continue_at)
            }
        }
    }
}
