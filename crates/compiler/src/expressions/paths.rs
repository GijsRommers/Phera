//! Mixed array and property paths preserve evaluation order and shared object identity.

use crate::ir::PathStep as Step;
use crate::ir::{Constant, Place, PlaceScope};
use crate::{CompileError, Compiler};
use phera_ast as ast;

impl Compiler<'_> {
    /// Lower every postfix access into one path, including object roots produced by calls.
    pub(crate) fn place(&mut self, expression: &ast::Expression) -> Result<Place, CompileError> {
        if crate::expressions::nullsafe::contains_nullsafe(expression) {
            return Err(self.error(
                "Can't use nullsafe operator in write context",
                self.lines.line(expression.span.start),
            ));
        }
        if matches!(&expression.kind, ast::ExpressionKind::Variable(ast::Variable::Named(name))
            if name == "this")
        {
            return Err(self.error(
                "Cannot re-assign $this",
                self.lines.line(expression.span.start),
            ));
        }
        match expression.kind {
            ast::ExpressionKind::Call { .. } => {
                return Err(self.error(
                    "Can't use function return value in write context",
                    self.lines.line(expression.span.start),
                ));
            }
            ast::ExpressionKind::MethodCall { .. } | ast::ExpressionKind::StaticCall { .. } => {
                return Err(self.error(
                    "Can't use method return value in write context",
                    self.lines.line(expression.span.start),
                ));
            }
            _ => {}
        }
        let mut root = expression;
        while let ast::ExpressionKind::Index { target, .. }
        | ast::ExpressionKind::Property { target, .. } = &root.kind
        {
            root = target;
        }
        if matches!(root.kind, ast::ExpressionKind::ClassConstant { .. }) {
            return Err(self.error(
                "Cannot use temporary expression in write context",
                self.lines.line(expression.span.start),
            ));
        }
        self.readable_place(expression)
    }

    /// Evaluate the root first and each dynamic key once in source order.
    pub(crate) fn readable_place(
        &mut self,
        expression: &ast::Expression,
    ) -> Result<Place, CompileError> {
        let line = self.lines.line(expression.span.start);
        let mut node = expression;
        let mut postfix = Vec::new();
        while let ast::ExpressionKind::Index { target, .. }
        | ast::ExpressionKind::Property { target, .. } = &node.kind
        {
            // The rest of a long chain is compiled already (see `chains`).
            if self.is_compiled_operand(node) {
                break;
            }
            self.note_hook_backing(node);
            postfix.push(node);
            node = target;
        }
        let (base, scope) = match &node.kind {
            ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name == "this" => {
                (self.allocate_register()?, PlaceScope::This)
            }

            ast::ExpressionKind::StaticProperty { class, name } => {
                let class = if matches!(class.kind, ast::ExpressionKind::Name(_)) {
                    let class = self.object_class_name(class, line)?;
                    let destination = self.allocate_register()?;
                    self.emit(
                        crate::ir::Instruction::ClassName { destination, class },
                        line,
                    );
                    destination
                } else {
                    self.compile_expression(class)?
                };
                let name = match name {
                    ast::Variable::Named(name) => {
                        self.literal(Constant::Bytes(name.as_bytes().to_vec()), line)?
                    }
                    ast::Variable::Dynamic(name) => self.compile_expression(name)?,
                };
                (
                    class,
                    PlaceScope::StaticProperty {
                        name,
                        scratch: self.allocate_register()?,
                    },
                )
            }

            ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name == "GLOBALS" => {
                (self.allocate_register()?, PlaceScope::Globals)
            }
            ast::ExpressionKind::Variable(ast::Variable::Named(name))
                if matches!(
                    name.as_str(),
                    "_GET"
                        | "_POST"
                        | "_COOKIE"
                        | "_SERVER"
                        | "_REQUEST"
                        | "_FILES"
                        | "_ENV"
                        | "_SESSION"
                ) =>
            {
                (
                    self.literal(Constant::Bytes(name.as_bytes().to_vec()), line)?,
                    PlaceScope::Global(self.allocate_register()?),
                )
            }
            ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name != "this" => {
                (self.local(name)?, PlaceScope::Local)
            }
            ast::ExpressionKind::Variable(ast::Variable::Dynamic(name)) => {
                self.function.dynamic_scope = true;
                (self.compile_expression(name)?, PlaceScope::Dynamic)
            }
            _ => (self.compile_expression(node)?, PlaceScope::Local),
        };
        let mut place = Place {
            base,
            scope,
            path: Vec::new(),
        };
        for access in postfix.into_iter().rev() {
            let step = match &access.kind {
                ast::ExpressionKind::Index { index, .. } => match index {
                    Some(index) => Step::Index(self.compile_expression(index)?),
                    None => Step::Append,
                },
                ast::ExpressionKind::Property { name, .. } => match name {
                    ast::MemberName::Identifier(name) => Step::Property(name.name.clone()),
                    ast::MemberName::Expression(name) => {
                        Step::DynamicProperty(self.compile_expression(name)?)
                    }
                },
                _ => return Err(self.error("Invalid access path", line)),
            };
            if place.scope == PlaceScope::Globals && place.path.is_empty() {
                let Step::Index(name) = step else {
                    return Err(self.error("Cannot use [] for reading", line));
                };
                place.scope = PlaceScope::Global(place.base);
                place.base = name;
            } else {
                place.path.push(step);
            }
        }
        Ok(place)
    }
}
