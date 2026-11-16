//! Bytecode lowering for expressions.
use super::*;
impl Compiler<'_> {
    pub(super) fn expr(&mut self, e: &a::Expression) -> Result<Reg, CompileError> {
        if matches!(e.kind, a::ExpressionKind::Binary { .. }) {
            let mut spine = vec![];
            let mut node = e;
            while let a::ExpressionKind::Binary {
                operator,
                left,
                right,
            } = &node.kind
            {
                spine.push((*operator, right.as_ref(), self.lines.line(node.span.start)));
                node = left;
            }
            let mut lhs = if spine
                .last()
                .is_some_and(|(op, _, _)| *op == a::BinaryOperator::NullCoalesce)
            {
                self.quiet(node)?
            } else {
                self.expr(node)?
            };
            for (op, right, line) in spine.into_iter().rev() {
                let watermark = self.next;
                match op {
                    a::BinaryOperator::And
                    | a::BinaryOperator::Or
                    | a::BinaryOperator::NullCoalesce => {
                        let dst = lhs;
                        let j = if op == a::BinaryOperator::NullCoalesce {
                            let j = self.emit(
                                Instruction::BranchNull {
                                    value: lhs,
                                    target: Target(0),
                                },
                                line,
                            );
                            let end = self.emit(Instruction::Jump(Target(0)), line);
                            self.patch(j, self.pos());
                            end
                        } else {
                            self.emit(
                                Instruction::Branch {
                                    value: lhs,
                                    target: Target(0),
                                    when: op == a::BinaryOperator::Or,
                                },
                                line,
                            )
                        };
                        let src = self.expr(right)?;
                        self.emit(Instruction::Move { dst, src }, line);
                        self.patch(j, self.pos());
                        if op != a::BinaryOperator::NullCoalesce {
                            self.emit(
                                Instruction::Unary {
                                    dst,
                                    op: Unary::Bool,
                                    src: dst,
                                },
                                line,
                            );
                        }
                        lhs = dst;
                    }
                    a::BinaryOperator::Pipe => {
                        let name = self.expr(right)?;
                        let dst = lhs;
                        self.emit(
                            Instruction::Call {
                                dst,
                                name,
                                fallback: None,
                                arguments: vec![Argument {
                                    value: lhs,
                                    place: None,
                                    name: None,
                                    spread: false,
                                }],
                            },
                            line,
                        );
                        lhs = dst;
                    }
                    _ => {
                        let rhs = self.expr(right)?;
                        self.emit(
                            Instruction::Binary {
                                dst: lhs,
                                op: binary(op).ok_or_else(|| {
                                    self.error("Operator not supported yet", line)
                                })?,
                                lhs,
                                rhs,
                            },
                            line,
                        );
                    }
                }
                if self.locals.values().all(|r| (r.0 as usize) < watermark) {
                    self.next = watermark;
                }
            }
            return Ok(lhs);
        }
        self.expr_node(e)
    }
    pub(super) fn expr_node(&mut self, e: &a::Expression) -> Result<Reg, CompileError> {
        use a::ExpressionKind as E;
        let line = self.lines.line(e.span.start);
        if let Some(c) = constant(e) {
            return self.literal(c, line);
        }
        match &e.kind {
            E::Variable(_) | E::Index { .. } => {
                let place = self.readable_place(e)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst,
                        place,
                        quiet: false,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Assign { target, value } => {
                let src = self.expr(value)?;
                self.assign(target, src, line)?;
                Ok(src)
            }
            E::AssignReference { target, value } => {
                let place = self.place(target)?;
                let source = self.place(value)?;
                self.emit(
                    Instruction::Bind {
                        place: place.clone(),
                        source,
                    },
                    line,
                );
                let dst = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst,
                        place,
                        quiet: false,
                    },
                    line,
                );
                Ok(dst)
            }
            E::CompoundAssign {
                operator,
                target,
                value,
            } => {
                let place = self.place(target)?;
                let lhs = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst: lhs,
                        place: place.clone(),
                        quiet: *operator == a::BinaryOperator::NullCoalesce,
                    },
                    line,
                );
                if *operator == a::BinaryOperator::NullCoalesce {
                    let j = self.emit(
                        Instruction::BranchNull {
                            value: lhs,
                            target: Target(0),
                        },
                        line,
                    );
                    let end = self.emit(Instruction::Jump(Target(0)), line);
                    self.patch(j, self.pos());
                    let rhs = self.expr(value)?;
                    self.emit(Instruction::Write { place, src: rhs }, line);
                    self.emit(Instruction::Move { dst: lhs, src: rhs }, line);
                    self.patch(end, self.pos());
                    return Ok(lhs);
                }
                let rhs = self.expr(value)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Binary {
                        dst,
                        op: binary(*operator)
                            .ok_or_else(|| self.error("Operator not supported yet", line))?,
                        lhs,
                        rhs,
                    },
                    line,
                );
                self.emit(Instruction::Write { place, src: dst }, line);
                Ok(dst)
            }
            E::IncDec {
                target,
                increment,
                prefix,
            } => {
                let place = self.place(target)?;
                let old = self.reg()?;
                self.emit(
                    Instruction::Read {
                        dst: old,
                        place: place.clone(),
                        quiet: false,
                    },
                    line,
                );
                let dst = self.reg()?;
                self.emit(
                    Instruction::Increment {
                        dst,
                        src: old,
                        increment: *increment,
                    },
                    line,
                );
                self.emit(Instruction::Write { place, src: dst }, line);
                Ok(if *prefix { dst } else { old })
            }
            E::Binary {
                operator: a::BinaryOperator::Pipe,
                left,
                right,
            } => {
                let value = self.expr(left)?;
                let name = self.expr(right)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Call {
                        dst,
                        name,
                        fallback: None,
                        arguments: vec![Argument {
                            value,
                            place: None,
                            name: None,
                            spread: false,
                        }],
                    },
                    line,
                );
                Ok(dst)
            }
            E::Binary {
                operator,
                left,
                right,
            } => {
                let lhs = if *operator == a::BinaryOperator::NullCoalesce {
                    self.quiet(left)?
                } else {
                    self.expr(left)?
                };
                let dst = self.reg()?;
                self.emit(Instruction::Move { dst, src: lhs }, line);
                let j = if *operator == a::BinaryOperator::NullCoalesce {
                    let j = self.emit(
                        Instruction::BranchNull {
                            value: lhs,
                            target: Target(0),
                        },
                        line,
                    );
                    let end = self.emit(Instruction::Jump(Target(0)), line);
                    self.patch(j, self.pos());
                    end
                } else {
                    self.emit(
                        Instruction::Branch {
                            value: lhs,
                            target: Target(0),
                            when: *operator == a::BinaryOperator::Or,
                        },
                        line,
                    )
                };
                let rhs = self.expr(right)?;
                self.emit(Instruction::Move { dst, src: rhs }, line);
                self.patch(j, self.pos());
                if *operator != a::BinaryOperator::NullCoalesce {
                    self.emit(
                        Instruction::Unary {
                            dst,
                            op: Unary::Bool,
                            src: dst,
                        },
                        line,
                    );
                }
                Ok(dst)
            }
            E::Unary { operator, operand } => {
                if *operator == a::UnaryOperator::ErrorSuppress {
                    self.emit(Instruction::Suppress(true), line);
                    let r = self.expr(operand)?;
                    self.emit(Instruction::Suppress(false), line);
                    return Ok(r);
                }
                let src = self.expr(operand)?;
                let dst = self.reg()?;
                let op = match operator {
                    a::UnaryOperator::Plus => Unary::Plus,
                    a::UnaryOperator::Minus => Unary::Minus,
                    a::UnaryOperator::Not => Unary::Not,
                    _ => Unary::BitwiseNot,
                };
                self.emit(Instruction::Unary { dst, op, src }, line);
                Ok(dst)
            }
            E::Cast { kind, operand } => {
                let op = match kind {
                    a::CastKind::Int => Unary::Int,
                    a::CastKind::Float => Unary::Float,
                    a::CastKind::String => Unary::String,
                    a::CastKind::Bool => Unary::Bool,
                    a::CastKind::Array => Unary::Array,
                    _ => return Err(self.error("Cast not supported yet", line)),
                };
                let src = self.expr(operand)?;
                let dst = self.reg()?;
                self.emit(Instruction::Unary { dst, op, src }, line);
                Ok(dst)
            }
            E::Ternary {
                condition,
                then,
                otherwise,
            } => {
                let c = self.expr(condition)?;
                let dst = self.reg()?;
                self.emit(Instruction::Move { dst, src: c }, line);
                let j = self.emit(
                    Instruction::Branch {
                        value: c,
                        target: Target(0),
                        when: false,
                    },
                    line,
                );
                if let Some(t) = then {
                    let src = self.expr(t)?;
                    self.emit(Instruction::Move { dst, src }, line);
                }
                let end = self.emit(Instruction::Jump(Target(0)), line);
                self.patch(j, self.pos());
                let src = self.expr(otherwise)?;
                self.emit(Instruction::Move { dst, src }, line);
                self.patch(end, self.pos());
                Ok(dst)
            }
            E::Include { kind, path } => {
                self.fun.dynamic_scope = true;
                let path = self.expr(path)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Include {
                        dst,
                        path,
                        once: matches!(
                            kind,
                            a::IncludeKind::IncludeOnce | a::IncludeKind::RequireOnce
                        ),
                        required: matches!(
                            kind,
                            a::IncludeKind::Require | a::IncludeKind::RequireOnce
                        ),
                    },
                    line,
                );
                Ok(dst)
            }
            E::Print(e) => {
                let r = self.expr(e)?;
                self.emit(Instruction::Echo(r), line);
                self.literal(Constant::Int(1), line)
            }
            E::Exit(e) => {
                let r = if let Some(e) = e {
                    self.expr(e)?
                } else {
                    self.literal(Constant::Int(0), line)?
                };
                self.emit(Instruction::Exit(r), line);
                Ok(r)
            }
            E::Name(n) => {
                let (name, fallback) = self.resolve(n, 2);
                let dst = self.reg()?;
                self.emit(
                    Instruction::Constant {
                        dst,
                        name,
                        fallback,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Call { callee, arguments } => {
                if let E::Name(n) = &callee.kind
                    && matches!(
                        n.text.to_ascii_lowercase().as_str(),
                        "extract" | "compact" | "get_defined_vars"
                    )
                {
                    self.fun.dynamic_scope = true;
                }
                let (name, fallback) = if let E::Name(n) = &callee.kind {
                    let (n, f) = self.resolve(n, 1);
                    (self.literal(Constant::Bytes(n.into_bytes()), line)?, f)
                } else {
                    (self.expr(callee)?, None)
                };
                let a::Arguments::List(args) = arguments else {
                    let dst = self.reg()?;
                    self.emit(
                        Instruction::Callable {
                            dst,
                            name,
                            fallback,
                        },
                        line,
                    );
                    return Ok(dst);
                };
                let mut arguments = vec![];
                for arg in args {
                    let place = if writable(&arg.value) {
                        Some(self.place(&arg.value)?)
                    } else {
                        None
                    };
                    let value = if let Some(p) = &place {
                        let dst = self.reg()?;
                        self.emit(
                            Instruction::ReadArgument {
                                dst,
                                place: p.clone(),
                            },
                            line,
                        );
                        dst
                    } else {
                        self.expr(&arg.value)?
                    };
                    arguments.push(Argument {
                        value,
                        place,
                        name: arg.name.as_ref().map(|n| n.name.clone()),
                        spread: arg.spread,
                    });
                }
                let dst = self.reg()?;
                self.emit(
                    Instruction::Call {
                        dst,
                        name,
                        fallback,
                        arguments,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Array(items) => {
                let dst = self.reg()?;
                self.emit(Instruction::Array(dst), line);
                for item in items {
                    let key = item.key.as_ref().map(|k| self.expr(k)).transpose()?;
                    let value = if item.by_reference {
                        let place = self.place(&item.value)?;
                        let dst = self.reg()?;
                        self.emit(Instruction::Reference { dst, place }, line);
                        dst
                    } else {
                        self.expr(&item.value)?
                    };
                    self.emit(
                        Instruction::ArrayPush {
                            array: dst,
                            key,
                            value,
                            spread: item.spread,
                        },
                        line,
                    );
                }
                Ok(dst)
            }
            E::InterpolatedString(parts) => {
                let mut dst = self.literal(Constant::Bytes(vec![]), line)?;
                for part in parts {
                    let rhs = match part {
                        a::StringPart::Literal(v) => {
                            self.literal(Constant::Bytes(v.clone()), line)?
                        }
                        a::StringPart::Expression(e) => self.expr(e)?,
                    };
                    let next = self.reg()?;
                    self.emit(
                        Instruction::Binary {
                            dst: next,
                            op: Binary::Concat,
                            lhs: dst,
                            rhs,
                        },
                        line,
                    );
                    dst = next;
                }
                Ok(dst)
            }
            E::Empty(v) => {
                let src = self.quiet(v)?;
                let dst = self.reg()?;
                self.emit(
                    Instruction::Unary {
                        dst,
                        op: Unary::Not,
                        src,
                    },
                    line,
                );
                Ok(dst)
            }
            E::Isset(values) => {
                let dst = self.literal(Constant::Bool(false), line)?;
                let mut ends = vec![];
                for v in values {
                    let r = self.quiet(v)?;
                    ends.push(self.emit(
                        Instruction::BranchNull {
                            value: r,
                            target: Target(0),
                        },
                        line,
                    ));
                }
                let src = self.literal(Constant::Bool(true), line)?;
                self.emit(Instruction::Move { dst, src }, line);
                for j in ends {
                    self.patch(j, self.pos())
                }
                Ok(dst)
            }
            E::Match(m) => {
                let subject = self.expr(&m.subject)?;
                let dst = self.reg()?;
                let mut done = vec![];
                let mut default = None;
                for arm in &m.arms {
                    if let Some(conditions) = &arm.conditions {
                        let mut jumps = vec![];
                        for c in conditions {
                            let rhs = self.expr(c)?;
                            let test = self.reg()?;
                            self.emit(
                                Instruction::Binary {
                                    dst: test,
                                    op: Binary::Identical,
                                    lhs: subject,
                                    rhs,
                                },
                                line,
                            );
                            jumps.push(self.emit(
                                Instruction::Branch {
                                    value: test,
                                    target: Target(0),
                                    when: true,
                                },
                                line,
                            ));
                        }
                        let skip = self.emit(Instruction::Jump(Target(0)), line);
                        for j in jumps {
                            self.patch(j, self.pos())
                        }
                        let src = self.expr(&arm.body)?;
                        self.emit(Instruction::Move { dst, src }, line);
                        done.push(self.emit(Instruction::Jump(Target(0)), line));
                        self.patch(skip, self.pos());
                    } else {
                        default = Some(&arm.body)
                    }
                }
                if let Some(e) = default {
                    let src = self.expr(e)?;
                    self.emit(Instruction::Move { dst, src }, line);
                } else {
                    self.emit(
                        Instruction::Fatal {
                            class: "UnhandledMatchError".into(),
                            message: "Unhandled match case".into(),
                        },
                        line,
                    );
                }
                for j in done {
                    self.patch(j, self.pos())
                }
                Ok(dst)
            }
            E::MagicConstant(m) => self.literal(
                match m {
                    a::MagicConstant::Line => Constant::Int(line as i64),
                    a::MagicConstant::File => Constant::Bytes(self.file.as_bytes().to_vec()),
                    a::MagicConstant::Dir => Constant::Bytes(
                        std::path::Path::new(self.file)
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_string_lossy()
                            .as_bytes()
                            .to_vec(),
                    ),
                    a::MagicConstant::Function | a::MagicConstant::Method => {
                        Constant::Bytes(if self.fun.name == "{main}" {
                            vec![]
                        } else {
                            self.fun.name.as_bytes().to_vec()
                        })
                    }
                    a::MagicConstant::Namespace => {
                        Constant::Bytes(self.namespace.as_bytes().to_vec())
                    }
                    _ => Constant::Bytes(vec![]),
                },
                line,
            ),
            _ => Err(self.error("Expression not supported yet", line)),
        }
    }
    pub(super) fn assign(
        &mut self,
        target: &a::Expression,
        src: Reg,
        line: usize,
    ) -> Result<(), CompileError> {
        if let a::ExpressionKind::List(items) = &target.kind {
            for (i, item) in items.iter().enumerate() {
                if let Some(item) = item {
                    if item.by_reference {
                        return Err(self.error("Destructuring references not supported yet", line));
                    }
                    let key = if let Some(k) = &item.key {
                        self.expr(k)?
                    } else {
                        self.literal(Constant::Int(i as i64), line)?
                    };
                    let dst = self.reg()?;
                    self.emit(
                        Instruction::Read {
                            dst,
                            place: Place {
                                base: src,
                                dynamic: false,
                                path: vec![Some(key)],
                            },
                            quiet: false,
                        },
                        line,
                    );
                    self.assign(&item.value, dst, line)?;
                }
            }
        } else {
            let place = self.place(target)?;
            self.emit(Instruction::Write { place, src }, line);
        }
        Ok(())
    }
    pub(super) fn quiet(&mut self, e: &a::Expression) -> Result<Reg, CompileError> {
        if matches!(
            e.kind,
            a::ExpressionKind::Variable(_) | a::ExpressionKind::Index { .. }
        ) {
            let place = self.readable_place(e)?;
            let dst = self.reg()?;
            self.emit(
                Instruction::Read {
                    dst,
                    place,
                    quiet: true,
                },
                self.lines.line(e.span.start),
            );
            Ok(dst)
        } else {
            self.expr(e)
        }
    }
}
