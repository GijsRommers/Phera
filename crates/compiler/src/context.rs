//! Compilation state and register allocation shared by AST lowering passes.
//!
//! AST nodes lower to compiler IR here; final interning belongs to `lower`.

use std::collections::HashMap;

use phera_ast as ast;
use phera_bytecode::Reg;
use phera_source::LineTable;

use crate::ir::{Constant, Function, Instruction, Place, Target};
use crate::{CompileError, writable};

pub(super) struct Loop {
    pub(super) breaks: Vec<usize>,
    pub(super) continues: Vec<usize>,
}
pub(super) struct Compiler<'ast> {
    pub(super) file: &'ast str,
    pub(super) lines: &'ast LineTable,
    pub(super) fun: Function,
    pub(super) functions: Vec<Function>,
    pub(super) hoisted: Vec<usize>,
    pub(super) next: usize,
    pub(super) peak: usize,
    pub(super) locals: HashMap<String, Reg>,
    pub(super) loops: Vec<Loop>,
    pub(super) namespace: String,
    pub(super) imports: HashMap<(u8, String), String>,
}
impl<'ast> Compiler<'ast> {
    pub(super) fn new(file: &'ast str, lines: &'ast LineTable, name: &str, ns: &str) -> Self {
        Self {
            file,
            lines,
            fun: Function {
                name: name.into(),
                ..Function::default()
            },
            functions: vec![],
            hoisted: vec![],
            next: 0,
            peak: 0,
            locals: HashMap::new(),
            loops: vec![],
            namespace: ns.into(),
            imports: HashMap::new(),
        }
    }
    pub(super) fn error(&self, message: impl Into<String>, line: usize) -> CompileError {
        CompileError {
            message: message.into(),
            file: self.file.into(),
            line,
            parse: false,
        }
    }
    pub(super) fn reg(&mut self) -> Result<Reg, CompileError> {
        if self.next >= u16::MAX as usize - 1 {
            return Err(self.error("Phera register limit exceeded", 1));
        }
        let index =
            u16::try_from(self.next).map_err(|_| self.error("Phera register limit exceeded", 1))?;
        self.next += 1;
        self.peak = self.peak.max(self.next);
        Ok(Reg(index))
    }
    pub(super) fn local(&mut self, name: &str) -> Result<Reg, CompileError> {
        if let Some(register) = self.locals.get(name) {
            return Ok(*register);
        }
        let register = self.reg()?;
        self.locals.insert(name.into(), register);
        self.fun.locals.push(name.into());
        Ok(register)
    }
    pub(super) fn finish(&mut self) -> Function {
        let mut mapping = vec![Reg(0); self.peak];
        let mut named = vec![false; self.peak];
        for (i, name) in self.fun.locals.iter().enumerate() {
            if let Some(old) = self.locals.get(name) {
                if let Some(slot) = mapping.get_mut(old.0 as usize) {
                    *slot = Reg(i as u16)
                }
                if let Some(slot) = named.get_mut(old.0 as usize) {
                    *slot = true
                }
            }
        }
        let mut next = self.fun.locals.len();
        for (i, is_named) in named.into_iter().enumerate() {
            if !is_named {
                if let Some(slot) = mapping.get_mut(i) {
                    *slot = Reg(next as u16)
                }
                next += 1;
            }
        }
        for instruction in &mut self.fun.instructions {
            instruction.map_registers(|register| {
                mapping
                    .get(register.0 as usize)
                    .copied()
                    .unwrap_or(register)
            });
        }
        self.fun.registers = self.peak;
        std::mem::take(&mut self.fun)
    }
    pub(super) fn emit(&mut self, i: Instruction, line: usize) -> usize {
        let index = self.fun.instructions.len();
        self.fun.instructions.push(i);
        self.fun.lines.push(line);
        index
    }
    pub(super) fn pos(&self) -> usize {
        self.fun.instructions.len()
    }
    pub(super) fn patch(&mut self, i: usize, to: usize) {
        if let Some(
            Instruction::DefaultCheck { passed: t, .. }
            | Instruction::StaticCheck { initialized: t, .. }
            | Instruction::Jump(t)
            | Instruction::Branch { target: t, .. }
            | Instruction::BranchNull { target: t, .. }
            | Instruction::IterNext { end: t, .. },
        ) = self.fun.instructions.get_mut(i)
        {
            *t = Target(to)
        }
    }
    pub(super) fn literal(&mut self, v: Constant, line: usize) -> Result<Reg, CompileError> {
        let dst = self.reg()?;
        let constant = self.fun.constants.len();
        self.fun.constants.push(v);
        self.emit(Instruction::Load { dst, constant }, line);
        Ok(dst)
    }
    pub(super) fn resolve(&self, index: &ast::Name, kind: u8) -> (String, Option<String>) {
        if index.kind == ast::NameKind::FullyQualified {
            return (index.text.clone(), None);
        }
        if index.kind != ast::NameKind::Relative {
            let (first, rest) = index.text.split_once('\\').unwrap_or((&index.text, ""));
            if let Some(v) = self.imports.get(&(
                if rest.is_empty() { kind } else { 0 },
                first.to_ascii_lowercase(),
            )) {
                return (
                    if rest.is_empty() {
                        v.clone()
                    } else {
                        format!("{v}\\{rest}")
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
    pub(super) fn place(&mut self, error: &ast::Expression) -> Result<Place, CompileError> {
        let line = self.lines.line(error.span.start);
        let mut node = error;
        let mut path = vec![];
        while let ast::ExpressionKind::Index { target, index } = &node.kind {
            path.push(index.as_deref());
            node = target;
        }
        let (base, dynamic) = match &node.kind {
            ast::ExpressionKind::Variable(ast::Variable::Named(index)) => {
                (self.local(index)?, false)
            }
            ast::ExpressionKind::Variable(ast::Variable::Dynamic(error)) => {
                self.fun.dynamic_scope = true;
                (self.expr(error)?, true)
            }
            _ => return Err(self.error("Expression is not a writable place", line)),
        };
        let mut steps = vec![];
        for index in path.into_iter().rev() {
            steps.push(index.map(|error| self.expr(error)).transpose()?)
        }
        Ok(Place {
            base,
            dynamic,
            path: steps,
        })
    }
    pub(super) fn readable_place(
        &mut self,
        error: &ast::Expression,
    ) -> Result<Place, CompileError> {
        if writable(error) {
            return self.place(error);
        }
        let mut node = error;
        let mut indices = vec![];
        while let ast::ExpressionKind::Index { target, index } = &node.kind {
            indices.push(index.as_deref());
            node = target;
        }
        let base = self.expr(node)?;
        let mut path = vec![];
        for index in indices.into_iter().rev() {
            path.push(index.map(|error| self.expr(error)).transpose()?)
        }
        Ok(Place {
            base,
            dynamic: false,
            path,
        })
    }
    pub(super) fn block(
        &mut self,
        statements: &[ast::Statement],
        hoist: bool,
    ) -> Result<(), CompileError> {
        for statement in statements {
            self.statement(statement, hoist)?
        }
        Ok(())
    }
    pub(super) fn loop_end(&mut self, continue_at: usize) {
        if let Some(l) = self.loops.pop() {
            let end = self.pos();
            for i in l.breaks {
                self.patch(i, end)
            }
            for i in l.continues {
                self.patch(i, continue_at)
            }
        }
    }
}
