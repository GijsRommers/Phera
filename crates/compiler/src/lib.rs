//! AST lowering on the parser's dedicated stack. Long expression spines use a work list.
mod functions;
mod helpers;
use helpers::{binary, constant, type_name, writable};
mod arrays;
mod calls;
mod errors;
mod expressions;
mod ir;
mod lower;
mod lowering;
mod matching;
mod optimize;
mod places;
mod statements;
use ir::{CompiledUnit, Constant, Function, Instruction, Place, Target};
use phera_ast as ast;
use phera_bytecode::Reg;
use phera_source::LineTable;
use std::collections::HashMap;
#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub file: String,
    pub line: usize,
    pub parse: bool,
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, function: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            function,
            "\n{}: {} in {} on line {}\n",
            if self.parse {
                "Parse error"
            } else {
                "Fatal error"
            },
            self.message,
            self.file,
            self.line
        )
    }
}
impl std::error::Error for CompileError {}
pub fn compile(source: &str, file: &str) -> Result<phera_bytecode::CompiledUnit, CompileError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-compiler".into())
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, || {
                let lines = LineTable::new(source);
                let ast =
                    phera_parser::parse_on_current_stack(source).map_err(|error| CompileError {
                        message: errors::parse_message(
                            &error.message,
                            source,
                            source
                                .get(error.span.start..error.span.end)
                                .unwrap_or_default(),
                        ),
                        file: file.into(),
                        line: errors::parse_line(
                            &error.message,
                            source,
                            lines.line(error.span.start),
                        ),
                        parse: true,
                    })?;
                let mut compiler = Compiler::new(file, &lines, "{main}", "");
                compiler.fun.dynamic_scope = true;
                compiler.block(&ast.statements, true)?;
                let register = compiler.literal(Constant::Int(1), 1)?;
                compiler.emit(Instruction::Return(register), 1);
                Ok(lower::lower(CompiledUnit {
                    main: compiler.finish(),
                    functions: compiler.functions,
                    hoisted: compiler.hoisted,
                    file: file.into(),
                }))
            })
            .map_err(|error| CompileError {
                message: error.to_string(),
                file: file.into(),
                line: 1,
                parse: false,
            })?
            .join()
            .unwrap_or_else(|_| {
                Err(CompileError {
                    message: "Phera internal error: compiler thread failed".into(),
                    file: file.into(),
                    line: 1,
                    parse: false,
                })
            })
    })
}
struct Loop {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}
struct Compiler<'ast> {
    file: &'ast str,
    lines: &'ast LineTable,
    fun: Function,
    functions: Vec<Function>,
    hoisted: Vec<usize>,
    next: usize,
    peak: usize,
    locals: HashMap<String, Reg>,
    loops: Vec<Loop>,
    namespace: String,
    imports: HashMap<(u8, String), String>,
}
impl<'ast> Compiler<'ast> {
    fn new(file: &'ast str, lines: &'ast LineTable, name: &str, ns: &str) -> Self {
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
    fn error(&self, message: impl Into<String>, line: usize) -> CompileError {
        CompileError {
            message: message.into(),
            file: self.file.into(),
            line,
            parse: false,
        }
    }
    fn reg(&mut self) -> Result<Reg, CompileError> {
        if self.next >= u16::MAX as usize - 1 {
            return Err(self.error("Phera register limit exceeded", 1));
        }
        let index =
            u16::try_from(self.next).map_err(|_| self.error("Phera register limit exceeded", 1))?;
        self.next += 1;
        self.peak = self.peak.max(self.next);
        Ok(Reg(index))
    }
    fn local(&mut self, name: &str) -> Result<Reg, CompileError> {
        if let Some(register) = self.locals.get(name) {
            return Ok(*register);
        }
        let register = self.reg()?;
        self.locals.insert(name.into(), register);
        self.fun.locals.push(name.into());
        Ok(register)
    }
    fn finish(&mut self) -> Function {
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
    fn emit(&mut self, i: Instruction, line: usize) -> usize {
        let index = self.fun.instructions.len();
        self.fun.instructions.push(i);
        self.fun.lines.push(line);
        index
    }
    fn pos(&self) -> usize {
        self.fun.instructions.len()
    }
    fn patch(&mut self, i: usize, to: usize) {
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
    fn literal(&mut self, v: Constant, line: usize) -> Result<Reg, CompileError> {
        let dst = self.reg()?;
        let constant = self.fun.constants.len();
        self.fun.constants.push(v);
        self.emit(Instruction::Load { dst, constant }, line);
        Ok(dst)
    }
    fn resolve(&self, index: &ast::Name, kind: u8) -> (String, Option<String>) {
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
    fn place(&mut self, error: &ast::Expression) -> Result<Place, CompileError> {
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
    fn readable_place(&mut self, error: &ast::Expression) -> Result<Place, CompileError> {
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
    fn block(&mut self, statements: &[ast::Statement], hoist: bool) -> Result<(), CompileError> {
        for statement in statements {
            self.statement(statement, hoist)?
        }
        Ok(())
    }
    fn loop_end(&mut self, continue_at: usize) {
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

#[cfg(test)]
mod tests {
    use super::compile;
    use phera_bytecode::Reg;
    #[test]
    fn tiny_caller_stack() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let compiler = compile("<?php echo 42;", "test.php").expect("valid PHP");
                assert!(compiler.main.dynamic_scope);
            })
            .expect("thread")
            .join()
            .expect("join");
    }
    #[test]
    fn namespace_fallback_and_import() {
        let compiler = compile(
            "<?php namespace App; use function Util\\f as g; g();f();",
            "test.php",
        )
        .expect("valid PHP");
        assert!(
            compiler
                .symbols
                .iter()
                .any(|symbol| symbol.name == "util\\f")
        );
        assert!(
            compiler
                .symbols
                .iter()
                .any(|symbol| symbol.name == "app\\f" && symbol.fallback.as_deref() == Some("f"))
        );
    }
    #[test]
    fn locals_precede_temporaries() {
        let compiler = compile("<?php $a=1;$b=2;echo $a+$b;", "test.php").expect("valid PHP");
        assert_eq!(
            compiler
                .main
                .locals
                .iter()
                .map(|name| compiler.names[name.index()].as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(matches!(
            compiler.main.instructions.get(1),
            Some(phera_bytecode::Instruction::Assign { local: Reg(0), .. })
        ));
    }
}
