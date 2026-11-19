//! Destructuring, writable paths, and quiet reads.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Constant, Instruction, Place};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Destructure in item order, binding references only for reference list elements.
    pub(crate) fn assign(
        &mut self,
        target: &ast::Expression,
        src: Reg,
        line: usize,
    ) -> Result<(), CompileError> {
        if let ast::ExpressionKind::List(items) = &target.kind {
            self.declare_list_locals(items)?;
            for (i, item) in items.iter().enumerate() {
                if let Some(item) = item {
                    let mark = self.mark_temporaries();
                    let key = if let Some(key) = &item.key {
                        self.compile_expression(key)?
                    } else {
                        self.literal(Constant::Int(i as i64), line)?
                    };
                    let dst = self.allocate_register()?;
                    let place = Place {
                        base: src,
                        scope: crate::ir::PlaceScope::Local,
                        path: vec![crate::ir::PathStep::Index(key)],
                    };
                    if item.by_reference || contains_reference(&item.value) {
                        self.emit(
                            Instruction::Reference {
                                destination: dst,
                                place,
                            },
                            line,
                        );
                        if !item.by_reference {
                            self.assign(&item.value, dst, line)?;
                            self.emit(Instruction::Clear { register: dst }, line);
                            self.release_temporaries(mark);
                            continue;
                        }
                        let target = self.place(&item.value)?;
                        self.emit(
                            Instruction::Rebind {
                                place: target,
                                source: dst,
                            },
                            line,
                        );
                    } else {
                        self.emit(
                            Instruction::Read {
                                destination: dst,
                                place,
                                mode: phera_bytecode::FetchMode::Destructure,
                            },
                            line,
                        );
                        self.assign(&item.value, dst, line)?;
                    }
                    self.release_temporaries(mark);
                }
            }
        } else {
            let place = self.place(target)?;
            self.emit(Instruction::Write { place, source: src }, line);
        }
        Ok(())
    }

    /// Read variables and indexes without undefined-value warnings; evaluate other expressions normally.
    pub(crate) fn quiet(&mut self, expression: &ast::Expression) -> Result<Reg, CompileError> {
        self.quiet_access(expression, phera_bytecode::FetchMode::Quiet)
    }

    pub(crate) fn quiet_access(
        &mut self,
        expression: &ast::Expression,
        mode: phera_bytecode::FetchMode,
    ) -> Result<Reg, CompileError> {
        if matches!(&expression.kind, ast::ExpressionKind::Variable(ast::Variable::Named(name))
            if name == "this")
        {
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::ThisOrNull { destination },
                self.lines.line(expression.span.start),
            );
            return Ok(destination);
        }
        if matches!(
            expression.kind,
            ast::ExpressionKind::Variable(_)
                | ast::ExpressionKind::Index { .. }
                | ast::ExpressionKind::Property { .. }
                | ast::ExpressionKind::StaticProperty { .. }
        ) {
            let place = self.readable_place(expression)?;
            if place
                .path
                .iter()
                .any(|step| matches!(step, crate::ir::PathStep::Append))
            {
                return Err(self.error(
                    "Cannot use [] for reading",
                    self.lines.line(expression.span.start),
                ));
            }
            let dst = self.allocate_register()?;
            self.emit(
                Instruction::Read {
                    destination: dst,
                    place,
                    mode,
                },
                self.lines.line(expression.span.start),
            );
            Ok(dst)
        } else {
            self.compile_expression(expression)
        }
    }
}

impl Compiler<'_> {
    /// Gives every variable of a destructuring target its register before
    /// any item is compiled, in source order:
    ///
    /// ```php
    /// [[$a0, $b0], [$a1, $b1] /* ... */] = $rows;
    /// ```
    ///
    /// A new local always takes a register no earlier code used (see
    /// [`Compiler::local`]), so declaring each one between the items'
    /// temporaries would leave those temporaries' registers unused for good,
    /// and a wide target would run out of registers. Only for targets made of
    /// plain variables with literal keys or none: anything else may mention a
    /// variable of its own first, and the order of first mentions is the
    /// order `get_defined_vars()` reports.
    fn declare_list_locals(
        &mut self,
        items: &[Option<ast::ArrayItem>],
    ) -> Result<(), CompileError> {
        if !is_plain_target(items) {
            return Ok(());
        }
        let mut names = Vec::new();
        collect_in_source_order(items, &mut names);
        for name in names {
            self.local(name)?;
        }
        Ok(())
    }
}

/// Whether a destructuring target holds only plain variables (not `$this`)
/// and nested targets, with literal keys or none.
fn is_plain_target(items: &[Option<ast::ArrayItem>]) -> bool {
    let mut pending = vec![items];
    while let Some(items) = pending.pop() {
        for item in items.iter().flatten() {
            if item
                .key
                .as_ref()
                .is_some_and(|key| crate::constant(key).is_none())
            {
                return false;
            }
            match &item.value.kind {
                ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name != "this" => {}
                ast::ExpressionKind::List(inner) => pending.push(inner),
                _ => return false,
            }
        }
    }
    true
}

/// The variables of a destructuring target, left to right.
fn collect_in_source_order<'a>(items: &'a [Option<ast::ArrayItem>], names: &mut Vec<&'a String>) {
    let mut pending: Vec<std::slice::Iter<'a, Option<ast::ArrayItem>>> = vec![items.iter()];
    while let Some(iterator) = pending.last_mut() {
        let Some(item) = iterator.next() else {
            pending.pop();
            continue;
        };
        let Some(item) = item else { continue };
        match &item.value.kind {
            ast::ExpressionKind::Variable(ast::Variable::Named(name)) => names.push(name),
            ast::ExpressionKind::List(inner) => pending.push(inner.iter()),
            _ => {}
        }
    }
}

/// A nested reference requires every ancestor array to retain its original writable cell.
pub(crate) fn contains_reference(expression: &ast::Expression) -> bool {
    let ast::ExpressionKind::List(items) = &expression.kind else {
        return false;
    };
    items
        .iter()
        .flatten()
        .any(|item| item.by_reference || contains_reference(&item.value))
}
