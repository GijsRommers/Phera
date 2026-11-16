//! Bytecode lowering for statements.
use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Binary, Constant, Instruction, Target};
use crate::{CompileError, Compiler, Loop, constant, writable};
impl Compiler<'_> {
    pub(super) fn statement(
        &mut self,
        statement: &ast::Statement,
        hoist: bool,
    ) -> Result<(), CompileError> {
        use ast::StatementKind as S;
        let line = self.lines.line(statement.span.start);
        match &statement.kind {
            S::Expression(expression) => {
                let first = self.next;
                self.expr(expression)?;
                let temporaries: Vec<_> = (first..self.next)
                    .map(|index| Reg(index as u16))
                    .filter(|register| !self.locals.values().any(|local| local == register))
                    .collect();
                for register in temporaries {
                    self.emit(Instruction::Clear { register }, line);
                }
            }
            S::Echo(expressions) => {
                for expression in expressions {
                    let register = self.expr(expression)?;
                    self.emit(Instruction::Echo(register), line);
                }
            }
            S::InlineHtml(value) => {
                let register = self.literal(Constant::Bytes(value.as_bytes().to_vec()), line)?;
                self.emit(Instruction::Echo(register), line);
            }
            S::Block(body) => self.block(body, false)?,
            S::If {
                condition,
                then,
                otherwise,
            } => {
                let register = self.expr(condition)?;
                let jump = self.emit(
                    Instruction::Branch {
                        value: register,
                        target: Target(0),
                        when: false,
                    },
                    line,
                );
                self.block(then, false)?;
                let end = self.emit(Instruction::Jump(Target(0)), line);
                self.patch(jump, self.pos());
                if let Some(body) = otherwise {
                    self.block(body, false)?
                }
                self.patch(end, self.pos());
            }
            S::While { condition, body } => {
                let start = self.pos();
                let register = self.expr(condition)?;
                let end = self.emit(
                    Instruction::Branch {
                        value: register,
                        target: Target(0),
                        when: false,
                    },
                    line,
                );
                self.loops.push(Loop {
                    breaks: vec![],
                    continues: vec![],
                });
                self.block(body, false)?;
                self.emit(Instruction::Jump(Target(start)), line);
                self.patch(end, self.pos());
                self.loop_end(start);
            }
            S::DoWhile { body, condition } => {
                let start = self.pos();
                self.loops.push(Loop {
                    breaks: vec![],
                    continues: vec![],
                });
                self.block(body, false)?;
                let cont = self.pos();
                let register = self.expr(condition)?;
                self.emit(
                    Instruction::Branch {
                        value: register,
                        target: Target(start),
                        when: true,
                    },
                    line,
                );
                self.loop_end(cont);
            }
            S::For {
                init,
                condition,
                step,
                body,
            } => {
                for expression in init {
                    self.expr(expression)?;
                }
                let start = self.pos();
                let mut last = None;
                for expression in condition {
                    last = Some(self.expr(expression)?)
                }
                let end = last.map(|register| {
                    self.emit(
                        Instruction::Branch {
                            value: register,
                            target: Target(0),
                            when: false,
                        },
                        line,
                    )
                });
                self.loops.push(Loop {
                    breaks: vec![],
                    continues: vec![],
                });
                self.block(body, false)?;
                let cont = self.pos();
                for expression in step {
                    self.expr(expression)?;
                }
                self.emit(Instruction::Jump(Target(start)), line);
                if let Some(jump) = end {
                    self.patch(jump, self.pos())
                }
                self.loop_end(cont);
            }
            S::Break(name) | S::Continue(name) => {
                let is_break = matches!(statement.kind, S::Break(_));
                let depth = *name as usize;
                if depth == 0 || depth > self.loops.len() {
                    return Err(self.error(
                        if self.loops.is_empty() {
                            format!(
                                "'{}' not in the 'loop' or 'switch' context",
                                if is_break { "break" } else { "continue" }
                            )
                        } else {
                            format!("Cannot 'break' {name} levels")
                        },
                        line,
                    ));
                }
                let jump = self.emit(Instruction::Jump(Target(0)), line);
                let at = self.loops.len() - depth;
                if let Some(loop_state) = self.loops.get_mut(at) {
                    if is_break {
                        loop_state.breaks.push(jump)
                    } else {
                        loop_state.continues.push(jump)
                    }
                }
            }
            S::Return(expression) => {
                let source = if let Some(expression) = expression {
                    if self.fun.returns_reference && writable(expression) {
                        let place = self.place(expression)?;
                        let dst = self.reg()?;
                        self.emit(Instruction::Reference { dst, place }, line);
                        dst
                    } else {
                        self.expr(expression)?
                    }
                } else {
                    self.literal(Constant::Null, line)?
                };
                self.emit(
                    if self.fun.returns_reference {
                        Instruction::ReturnReference(source)
                    } else {
                        Instruction::Return(source)
                    },
                    line,
                );
            }
            S::Function(declaration) => self.compile_function(declaration, hoist, line)?,
            S::Namespace {
                name, statements, ..
            } => {
                let old = self.namespace.clone();
                self.namespace = name
                    .as_ref()
                    .map(|name| name.text.clone())
                    .unwrap_or_default();
                self.block(statements, hoist)?;
                self.namespace = old;
            }
            S::Use(items) => {
                for i in items {
                    let alias =
                        i.alias.as_ref().map(|x| x.name.clone()).unwrap_or_else(|| {
                            i.name.text.rsplit('\\').next().unwrap_or("").into()
                        });
                    let key = match i.kind {
                        ast::UseKind::Class => 0,
                        ast::UseKind::Function => 1,
                        ast::UseKind::Constant => 2,
                    };
                    self.imports
                        .insert((key, alias.to_ascii_lowercase()), i.name.text.clone());
                }
            }
            S::Const { items, .. } => {
                for i in items {
                    let register = self.expr(&i.value)?;
                    let name = if self.namespace.is_empty() {
                        i.name.name.clone()
                    } else {
                        format!("{}\\{}", self.namespace, i.name.name)
                    };
                    self.emit(
                        Instruction::Define {
                            name,
                            src: register,
                        },
                        line,
                    );
                }
            }
            S::Declare { directives, body } => {
                for directive in directives {
                    if directive.name.name == "strict_types" {
                        self.fun.strict_types =
                            matches!(constant(&directive.value), Some(Constant::Int(1)))
                    } else {
                        return Err(self.error(
                            format!("declare({}) not supported yet", directive.name.name),
                            line,
                        ));
                    }
                }
                if let Some(body) = body {
                    self.block(body, hoist)?
                }
            }
            S::Global(vars) => {
                for value in vars {
                    if let ast::Variable::Named(name) = value {
                        let local = self.local(name)?;
                        self.emit(
                            Instruction::Global {
                                local,
                                name: name.clone(),
                            },
                            line,
                        );
                    } else {
                        if let ast::Variable::Dynamic(expression) = value {
                            self.fun.dynamic_scope = true;
                            let name = self.expr(expression)?;
                            self.emit(Instruction::DynamicGlobal { name }, line);
                        }
                    }
                }
            }
            S::Static(vars) => {
                for var in vars {
                    let local = self.local(&var.name)?;
                    let check = self.emit(
                        Instruction::StaticCheck {
                            local,
                            name: var.name.clone(),
                            initialized: Target(0),
                        },
                        line,
                    );
                    let src = if let Some(default) = &var.default {
                        self.expr(default)?
                    } else {
                        self.literal(Constant::Null, line)?
                    };
                    self.emit(
                        Instruction::StaticInit {
                            local,
                            name: var.name.clone(),
                            src,
                        },
                        line,
                    );
                    self.patch(check, self.pos());
                }
            }
            S::Unset(expressions) => {
                for expression in expressions {
                    let parameter = self.place(expression)?;
                    self.emit(Instruction::Unset(parameter), line);
                }
            }
            S::Foreach {
                subject,
                key,
                value,
                by_reference,
                body,
            } => {
                let src = if *by_reference {
                    self.literal(Constant::Null, line)?
                } else {
                    self.expr(subject)?
                };
                let id = self.reg()?;
                let key_register = self.reg()?;
                let value_register = self.reg()?;
                let place = if *by_reference {
                    Some(self.place(subject)?)
                } else {
                    None
                };
                self.emit(Instruction::IterInit { id, src, place }, line);
                let start = self.pos();
                let jump = self.emit(
                    Instruction::IterNext {
                        id,
                        key: key_register,
                        value: value_register,
                        end: Target(0),
                    },
                    line,
                );
                if let Some(key) = key {
                    let place = self.place(key)?;
                    self.emit(
                        Instruction::Write {
                            place,
                            src: key_register,
                        },
                        line,
                    );
                }
                if *by_reference {
                    let place = self.place(value)?;
                    self.emit(
                        Instruction::Rebind {
                            place,
                            src: value_register,
                        },
                        line,
                    );
                } else {
                    self.assign(value, value_register, line)?;
                }
                self.loops.push(Loop {
                    breaks: vec![],
                    continues: vec![],
                });
                self.block(body, false)?;
                self.emit(Instruction::Jump(Target(start)), line);
                self.patch(jump, self.pos());
                self.loop_end(start);
            }
            S::Switch { subject, cases } => {
                let value = self.expr(subject)?;
                let mut jumps = vec![];
                let mut default = None;
                for (i, c) in cases.iter().enumerate() {
                    if let Some(expression) = &c.condition {
                        let rhs = self.expr(expression)?;
                        let dst = self.reg()?;
                        self.emit(
                            Instruction::Binary {
                                dst,
                                operator: Binary::Equal,
                                lhs: value,
                                rhs,
                            },
                            line,
                        );
                        jumps.push((
                            i,
                            self.emit(
                                Instruction::Branch {
                                    value: dst,
                                    target: Target(0),
                                    when: true,
                                },
                                line,
                            ),
                        ));
                    } else {
                        default = Some(i)
                    }
                }
                let miss = self.emit(Instruction::Jump(Target(0)), line);
                self.loops.push(Loop {
                    breaks: vec![],
                    continues: vec![],
                });
                for (i, c) in cases.iter().enumerate() {
                    for (_, jump) in jumps.iter().filter(|(name, _)| *name == i) {
                        self.patch(*jump, self.pos())
                    }
                    if default == Some(i) {
                        self.patch(miss, self.pos())
                    }
                    self.block(&c.body, false)?
                }
                if default.is_none() {
                    self.patch(miss, self.pos())
                }
                self.loop_end(self.pos());
            }
            _ => return Err(self.error("Statement not supported yet", line)),
        }
        Ok(())
    }
}
