//! Bytecode lowering for statements.
use super::*;
impl Compiler<'_> {
    pub(super) fn statement(&mut self, s: &a::Statement, hoist: bool) -> Result<(), CompileError> {
        use a::StatementKind as S;
        let line = self.lines.line(s.span.start);
        match &s.kind {
            S::Expression(e) => {
                self.expr(e)?;
            }
            S::Echo(es) => {
                for e in es {
                    let r = self.expr(e)?;
                    self.emit(Instruction::Echo(r), line);
                }
            }
            S::InlineHtml(v) => {
                let r = self.literal(Constant::Bytes(v.as_bytes().to_vec()), line)?;
                self.emit(Instruction::Echo(r), line);
            }
            S::Block(b) => self.block(b, false)?,
            S::If {
                condition,
                then,
                otherwise,
            } => {
                let r = self.expr(condition)?;
                let j = self.emit(
                    Instruction::Branch {
                        value: r,
                        target: Target(0),
                        when: false,
                    },
                    line,
                );
                self.block(then, false)?;
                let end = self.emit(Instruction::Jump(Target(0)), line);
                self.patch(j, self.pos());
                if let Some(b) = otherwise {
                    self.block(b, false)?
                }
                self.patch(end, self.pos());
            }
            S::While { condition, body } => {
                let start = self.pos();
                let r = self.expr(condition)?;
                let end = self.emit(
                    Instruction::Branch {
                        value: r,
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
                let r = self.expr(condition)?;
                self.emit(
                    Instruction::Branch {
                        value: r,
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
                for e in init {
                    self.expr(e)?;
                }
                let start = self.pos();
                let mut last = None;
                for e in condition {
                    last = Some(self.expr(e)?)
                }
                let end = last.map(|r| {
                    self.emit(
                        Instruction::Branch {
                            value: r,
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
                for e in step {
                    self.expr(e)?;
                }
                self.emit(Instruction::Jump(Target(start)), line);
                if let Some(j) = end {
                    self.patch(j, self.pos())
                }
                self.loop_end(cont);
            }
            S::Break(n) | S::Continue(n) => {
                let is_break = matches!(s.kind, S::Break(_));
                let depth = *n as usize;
                if depth == 0 || depth > self.loops.len() {
                    return Err(self.error(
                        if self.loops.is_empty() {
                            format!(
                                "'{}' not in the 'loop' or 'switch' context",
                                if is_break { "break" } else { "continue" }
                            )
                        } else {
                            format!("Cannot 'break' {n} levels")
                        },
                        line,
                    ));
                }
                let j = self.emit(Instruction::Jump(Target(0)), line);
                let at = self.loops.len() - depth;
                if let Some(l) = self.loops.get_mut(at) {
                    if is_break {
                        l.breaks.push(j)
                    } else {
                        l.continues.push(j)
                    }
                }
            }
            S::Return(e) => {
                let r = if let Some(e) = e {
                    self.expr(e)?
                } else {
                    self.literal(Constant::Null, line)?
                };
                self.emit(Instruction::Return(r), line);
            }
            S::Function(f) => {
                let name = if self.namespace.is_empty() {
                    f.name.name.clone()
                } else {
                    format!("{}\\{}", self.namespace, f.name.name)
                };
                if hoist
                    && self
                        .hoisted
                        .iter()
                        .filter_map(|i| self.functions.get(*i))
                        .any(|f| f.name.eq_ignore_ascii_case(&name))
                {
                    return Err(self.error(format!("Cannot redeclare function {name}()"), line));
                }
                let mut sub = Compiler::new(self.file, self.lines, &name, &self.namespace);
                sub.imports = self.imports.clone();
                sub.fun.strict_types = self.fun.strict_types;
                for p in &f.parameters {
                    sub.local(&p.name)?;
                    sub.fun.parameters.push(Parameter {
                        name: p.name.clone(),
                        default: p
                            .default
                            .as_ref()
                            .map(|e| {
                                constant(e).ok_or_else(|| {
                                    self.error("Non-literal default not supported yet", line)
                                })
                            })
                            .transpose()?,
                        ty: p.ty.as_ref().map(type_name),
                        by_reference: p.by_reference,
                        variadic: p.variadic,
                    });
                }
                sub.fun.return_type = f.return_type.as_ref().map(type_name);
                sub.block(&f.body, false)?;
                let r = sub.literal(Constant::Null, line)?;
                sub.emit(Instruction::Return(r), line);
                let id = self.functions.len();
                let mut function = sub.finish();
                for op in function.instructions.iter_mut().chain(
                    sub.functions
                        .iter_mut()
                        .flat_map(|f| f.instructions.iter_mut()),
                ) {
                    if let Instruction::Declare(i) = op {
                        *i += id + 1;
                    }
                }
                self.functions.push(function);
                self.functions.extend(sub.functions);
                if hoist {
                    self.hoisted.push(id)
                } else {
                    self.emit(Instruction::Declare(id), line);
                }
            }
            S::Namespace {
                name, statements, ..
            } => {
                let old = self.namespace.clone();
                self.namespace = name.as_ref().map(|n| n.text.clone()).unwrap_or_default();
                self.block(statements, hoist)?;
                self.namespace = old;
            }
            S::Use(items) => {
                for i in items {
                    let alias =
                        i.alias.as_ref().map(|x| x.name.clone()).unwrap_or_else(|| {
                            i.name.text.rsplit('\\').next().unwrap_or("").into()
                        });
                    let k = match i.kind {
                        a::UseKind::Class => 0,
                        a::UseKind::Function => 1,
                        a::UseKind::Constant => 2,
                    };
                    self.imports
                        .insert((k, alias.to_ascii_lowercase()), i.name.text.clone());
                }
            }
            S::Const { items, .. } => {
                for i in items {
                    let r = self.expr(&i.value)?;
                    let name = if self.namespace.is_empty() {
                        i.name.name.clone()
                    } else {
                        format!("{}\\{}", self.namespace, i.name.name)
                    };
                    self.emit(Instruction::Define { name, src: r }, line);
                }
            }
            S::Declare { directives, body } => {
                for d in directives {
                    if d.name.name == "strict_types" {
                        self.fun.strict_types = matches!(constant(&d.value), Some(Constant::Int(1)))
                    } else {
                        return Err(
                            self.error(format!("declare({}) not supported yet", d.name.name), line)
                        );
                    }
                }
                if let Some(b) = body {
                    self.block(b, hoist)?
                }
            }
            S::Global(vars) => {
                for v in vars {
                    if let a::Variable::Named(name) = v {
                        let local = self.local(name)?;
                        self.emit(
                            Instruction::Global {
                                local,
                                name: name.clone(),
                            },
                            line,
                        );
                    } else {
                        return Err(self.error("Dynamic global not supported yet", line));
                    }
                }
            }
            S::Static(vars) => {
                for var in vars {
                    let local = self.local(&var.name)?;
                    let default = if let Some(e) = &var.default {
                        constant(e).ok_or_else(|| {
                            self.error("Non-literal static initializer not supported yet", line)
                        })?
                    } else {
                        Constant::Null
                    };
                    self.emit(
                        Instruction::Static {
                            local,
                            name: var.name.clone(),
                            default,
                        },
                        line,
                    );
                }
            }
            S::Unset(es) => {
                for e in es {
                    let p = self.place(e)?;
                    self.emit(Instruction::Unset(p), line);
                }
            }
            S::Foreach {
                subject,
                key,
                value,
                by_reference,
                body,
            } => {
                let src = self.expr(subject)?;
                let id = self.reg()?;
                let k = self.reg()?;
                let v = self.reg()?;
                let place = if *by_reference {
                    Some(self.place(subject)?)
                } else {
                    None
                };
                self.emit(Instruction::IterInit { id, src, place }, line);
                let start = self.pos();
                let j = self.emit(
                    Instruction::IterNext {
                        id,
                        key: k,
                        value: v,
                        end: Target(0),
                    },
                    line,
                );
                if let Some(key) = key {
                    let place = self.place(key)?;
                    self.emit(Instruction::Write { place, src: k }, line);
                }
                if *by_reference {
                    let place = self.place(value)?;
                    self.emit(Instruction::Rebind { place, src: v }, line);
                } else {
                    self.assign(value, v, line)?;
                }
                self.loops.push(Loop {
                    breaks: vec![],
                    continues: vec![],
                });
                self.block(body, false)?;
                self.emit(Instruction::Jump(Target(start)), line);
                self.patch(j, self.pos());
                self.loop_end(start);
            }
            S::Switch { subject, cases } => {
                let v = self.expr(subject)?;
                let mut jumps = vec![];
                let mut default = None;
                for (i, c) in cases.iter().enumerate() {
                    if let Some(e) = &c.condition {
                        let rhs = self.expr(e)?;
                        let dst = self.reg()?;
                        self.emit(
                            Instruction::Binary {
                                dst,
                                op: Binary::Equal,
                                lhs: v,
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
                    for (_, j) in jumps.iter().filter(|(n, _)| *n == i) {
                        self.patch(*j, self.pos())
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
