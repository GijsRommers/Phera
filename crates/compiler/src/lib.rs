//! AST lowering on the parser's dedicated stack. Long expression spines use a work list.
mod expressions;
mod statements;
use phera_ast as a;
use phera_bytecode::*;
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
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
pub fn compile(source: &str, file: &str) -> Result<CompiledUnit, CompileError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-compiler".into())
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, || {
                let lines = LineTable::new(source);
                let ast =
                    phera_parser::parse_on_current_stack(source).map_err(|e| CompileError {
                        message: e.message,
                        file: file.into(),
                        line: lines.line(e.span.start),
                        parse: true,
                    })?;
                let mut c = Compiler::new(file, &lines, "{main}", "");
                c.fun.dynamic_scope = true;
                c.block(&ast.statements, true)?;
                let r = c.literal(Constant::Int(1), 1)?;
                c.emit(Instruction::Return(r), 1);
                Ok(CompiledUnit {
                    main: c.finish(),
                    functions: c.functions,
                    hoisted: c.hoisted,
                    constants: vec![],
                    file: file.into(),
                })
            })
            .map_err(|e| CompileError {
                message: e.to_string(),
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
struct Compiler<'a> {
    file: &'a str,
    lines: &'a LineTable,
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
impl<'a> Compiler<'a> {
    fn new(file: &'a str, lines: &'a LineTable, name: &str, ns: &str) -> Self {
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
        let n =
            u16::try_from(self.next).map_err(|_| self.error("Phera register limit exceeded", 1))?;
        self.next += 1;
        self.peak = self.peak.max(self.next);
        Ok(Reg(n))
    }
    fn local(&mut self, name: &str) -> Result<Reg, CompileError> {
        if let Some(r) = self.locals.get(name) {
            return Ok(*r);
        }
        let r = self.reg()?;
        self.locals.insert(name.into(), r);
        self.fun.locals.push(name.into());
        Ok(r)
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
        for op in &mut self.fun.instructions {
            op.map_registers(|r| mapping.get(r.0 as usize).copied().unwrap_or(r));
        }
        self.fun.registers = self.peak;
        std::mem::take(&mut self.fun)
    }
    fn emit(&mut self, i: Instruction, line: usize) -> usize {
        let n = self.fun.instructions.len();
        self.fun.instructions.push(i);
        self.fun.lines.push(line);
        n
    }
    fn pos(&self) -> usize {
        self.fun.instructions.len()
    }
    fn patch(&mut self, i: usize, to: usize) {
        if let Some(
            Instruction::Jump(t)
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
    fn resolve(&self, n: &a::Name, kind: u8) -> (String, Option<String>) {
        if n.kind == a::NameKind::FullyQualified {
            return (n.text.clone(), None);
        }
        if n.kind != a::NameKind::Relative {
            let (first, rest) = n.text.split_once('\\').unwrap_or((&n.text, ""));
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
            n.text.clone()
        } else {
            format!("{}\\{}", self.namespace, n.text)
        };
        let fallback = (n.kind == a::NameKind::Unqualified && !self.namespace.is_empty())
            .then(|| n.text.clone());
        (full, fallback)
    }
    fn place(&mut self, e: &a::Expression) -> Result<Place, CompileError> {
        let line = self.lines.line(e.span.start);
        let mut node = e;
        let mut path = vec![];
        while let a::ExpressionKind::Index { target, index } = &node.kind {
            path.push(index.as_deref());
            node = target;
        }
        let (base, dynamic) = match &node.kind {
            a::ExpressionKind::Variable(a::Variable::Named(n)) => (self.local(n)?, false),
            a::ExpressionKind::Variable(a::Variable::Dynamic(e)) => {
                self.fun.dynamic_scope = true;
                (self.expr(e)?, true)
            }
            _ => return Err(self.error("Expression is not a writable place", line)),
        };
        let mut steps = vec![];
        for index in path.into_iter().rev() {
            steps.push(index.map(|e| self.expr(e)).transpose()?)
        }
        Ok(Place {
            base,
            dynamic,
            path: steps,
        })
    }
    fn readable_place(&mut self, e: &a::Expression) -> Result<Place, CompileError> {
        if writable(e) {
            return self.place(e);
        }
        let mut node = e;
        let mut indices = vec![];
        while let a::ExpressionKind::Index { target, index } = &node.kind {
            indices.push(index.as_deref());
            node = target;
        }
        let base = self.expr(node)?;
        let mut path = vec![];
        for index in indices.into_iter().rev() {
            path.push(index.map(|e| self.expr(e)).transpose()?)
        }
        Ok(Place {
            base,
            dynamic: false,
            path,
        })
    }
    fn block(&mut self, ss: &[a::Statement], hoist: bool) -> Result<(), CompileError> {
        for s in ss {
            self.statement(s, hoist)?
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

fn constant(e: &a::Expression) -> Option<Constant> {
    match &e.kind {
        a::ExpressionKind::Array(items) if items.is_empty() => Some(Constant::EmptyArray),
        a::ExpressionKind::Unary {
            operator: a::UnaryOperator::Minus,
            operand,
        } => match constant(operand)? {
            Constant::Int(v) => v
                .checked_neg()
                .map(Constant::Int)
                .or(Some(Constant::Float(-(v as f64)))),
            Constant::Float(v) => Some(Constant::Float(-v)),
            _ => None,
        },
        a::ExpressionKind::Literal(l) => Some(match l {
            a::Literal::Null => Constant::Null,
            a::Literal::Bool(v) => Constant::Bool(*v),
            a::Literal::Int(v) => Constant::Int(*v),
            a::Literal::Float(v) => Constant::Float(*v),
            a::Literal::String(v) => Constant::Bytes(v.clone()),
        }),
        _ => None,
    }
}
fn type_name(t: &a::Type) -> String {
    match &t.kind {
        a::TypeKind::Named(n) => n.text.clone(),
        a::TypeKind::Nullable(t) => format!("?{}", type_name(t)),
        a::TypeKind::Union(ts) => ts.iter().map(type_name).collect::<Vec<_>>().join("|"),
        a::TypeKind::Intersection(ts) => ts.iter().map(type_name).collect::<Vec<_>>().join("&"),
    }
}
fn binary(op: a::BinaryOperator) -> Option<Binary> {
    use a::BinaryOperator as A;
    Some(match op {
        A::Add => Binary::Add,
        A::Subtract => Binary::Subtract,
        A::Multiply => Binary::Multiply,
        A::Divide => Binary::Divide,
        A::Modulo => Binary::Modulo,
        A::Power => Binary::Power,
        A::Concat => Binary::Concat,
        A::BitwiseAnd => Binary::BitwiseAnd,
        A::BitwiseOr => Binary::BitwiseOr,
        A::BitwiseXor => Binary::BitwiseXor,
        A::ShiftLeft => Binary::ShiftLeft,
        A::ShiftRight => Binary::ShiftRight,
        A::Xor => Binary::Xor,
        A::Equal => Binary::Equal,
        A::NotEqual => Binary::NotEqual,
        A::Identical => Binary::Identical,
        A::NotIdentical => Binary::NotIdentical,
        A::Less => Binary::Less,
        A::LessEqual => Binary::LessEqual,
        A::Greater => Binary::Greater,
        A::GreaterEqual => Binary::GreaterEqual,
        A::Spaceship => Binary::Spaceship,
        _ => return None,
    })
}
fn writable(e: &a::Expression) -> bool {
    let mut node = e;
    while let a::ExpressionKind::Index { target, .. } = &node.kind {
        node = target;
    }
    matches!(node.kind, a::ExpressionKind::Variable(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiny_caller_stack() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let c = compile("<?php echo 42;", "test.php").expect("valid PHP");
                assert!(c.main.dynamic_scope);
            })
            .expect("thread")
            .join()
            .expect("join");
    }
    #[test]
    fn namespace_fallback_and_import() {
        let c = compile(
            "<?php namespace App; use function Util\\f as g; g();f();",
            "test.php",
        )
        .expect("valid PHP");
        assert!(
            c.main
                .constants
                .contains(&Constant::Bytes(b"Util\\f".to_vec()))
        );
        assert!(
            c.main
                .constants
                .contains(&Constant::Bytes(b"App\\f".to_vec()))
        );
        assert!(
            c.main
                .instructions
                .iter()
                .any(|i| matches!(i,Instruction::Call{fallback:Some(n),..}if n=="f"))
        );
    }
    #[test]
    fn locals_precede_temporaries() {
        let c = compile("<?php $a=1;$b=2;echo $a+$b;", "test.php").expect("valid PHP");
        assert_eq!(c.main.locals, ["a", "b"]);
        assert!(matches!(
            c.main.instructions.get(1),
            Some(Instruction::Write {
                place: Place { base: Reg(0), .. },
                ..
            })
        ));
    }
}
