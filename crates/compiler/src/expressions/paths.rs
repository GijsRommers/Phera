//! Mixed array and property paths preserve evaluation order and shared object identity.

use crate::ir::PathStep as Step;
use crate::ir::{Constant, Instruction, Place, PlaceScope};
use crate::{CompileError, Compiler};
use phera_ast as ast;
use phera_bytecode::{FetchMode, Reg};

enum Access<'a> {
    Place,
    Read,
    Assignment(&'a mut Vec<DeferredIndex>),
}

pub(super) struct DeferredIndex {
    pub(super) source: Place,
    pub(super) destination: Reg,
    pub(super) line: usize,
}

impl Compiler<'_> {
    /// Lower every postfix access into one path, including object roots produced by calls.
    pub(crate) fn place(&mut self, expression: &ast::Expression) -> Result<Place, CompileError> {
        self.writable_place(expression, Access::Place)
    }

    pub(super) fn assignment_place(
        &mut self,
        expression: &ast::Expression,
        indexes: &mut Vec<DeferredIndex>,
    ) -> Result<Place, CompileError> {
        self.writable_place(expression, Access::Assignment(indexes))
    }

    fn writable_place(
        &mut self,
        expression: &ast::Expression,
        access: Access<'_>,
    ) -> Result<Place, CompileError> {
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
        self.access_path(expression, access).map(|(place, _)| place)
    }

    /// The place a reference is taken from: `[&$x]`, `$a = &$x`.
    ///
    /// `$this` cannot be rebound, so PHP takes the reference from a copy of
    /// it: `[&$this]` holds a reference to the object, and assigning through
    /// that reference leaves `$this` alone.
    pub(crate) fn reference_source(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<Place, CompileError> {
        if !crate::helpers::is_this(expression) {
            return self.place(expression);
        }
        let copy = self.allocate_register()?;
        self.emit(Instruction::This { destination: copy }, line);
        Ok(Place {
            base: copy,
            scope: PlaceScope::Local,
            path: Vec::new(),
        })
    }

    /// Evaluate the root first and each dynamic key once in source order.
    pub(crate) fn readable_place(
        &mut self,
        expression: &ast::Expression,
    ) -> Result<Place, CompileError> {
        self.access_path(expression, Access::Place)
            .map(|(place, _)| place)
    }

    /// Keep one fetch for same-line paths and give each multiline segment its own line.
    pub(crate) fn compile_read(
        &mut self,
        expression: &ast::Expression,
    ) -> Result<Reg, CompileError> {
        let (place, line) = self.access_path(expression, Access::Read)?;
        self.emit_path_read(place, line)
    }

    fn emit_path_read(&mut self, place: Place, line: usize) -> Result<Reg, CompileError> {
        if place.path.iter().any(|step| matches!(step, Step::Append)) {
            return Err(self.error("Cannot use [] for reading", line));
        }
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::Read {
                destination,
                place,
                mode: FetchMode::Read,
            },
            line,
        );
        Ok(destination)
    }

    pub(super) fn access_line(&self, access: &ast::Expression) -> usize {
        match &access.kind {
            ast::ExpressionKind::Property { name, .. } => self.member_line(name),
            ast::ExpressionKind::StaticProperty { name_start, .. } => self.lines.line(*name_start),
            ast::ExpressionKind::Index {
                index: Some(index), ..
            } => self.lines.line(index.span.end.saturating_sub(1)),
            _ => self.lines.line(access.span.start),
        }
    }

    fn access_path(
        &mut self,
        expression: &ast::Expression,
        mut usage: Access<'_>,
    ) -> Result<(Place, usize), CompileError> {
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
        let mut fetch_line = self.access_line(node);
        let (base, scope) = match &node.kind {
            ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name == "this" => {
                (self.allocate_register()?, PlaceScope::This)
            }

            ast::ExpressionKind::StaticProperty { class, name, .. } => {
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
            if matches!(usage, Access::Read) {
                let access_line = self.access_line(access);
                if fetch_line != access_line
                    && (!place.path.is_empty()
                        || matches!(place.scope, PlaceScope::StaticProperty { .. }))
                {
                    let base = self.emit_path_read(place, fetch_line)?;
                    place = Place {
                        base,
                        scope: PlaceScope::Local,
                        path: Vec::new(),
                    };
                }
                fetch_line = access_line;
            }
            let step = match &access.kind {
                ast::ExpressionKind::Index { index, .. } => match index {
                    Some(index) => {
                        if let Access::Assignment(indexes) = &mut usage
                            && matches!(
                                index.kind,
                                ast::ExpressionKind::Variable(ast::Variable::Named(_))
                            )
                        {
                            let source = self.readable_place(index)?;
                            if source.scope == PlaceScope::Local && source.path.is_empty() {
                                let destination = self.allocate_register()?;
                                indexes.push(DeferredIndex {
                                    source,
                                    destination,
                                    line: self.lines.line(index.span.start),
                                });
                                Step::Index(destination)
                            } else {
                                Step::Index(self.compile_expression(index)?)
                            }
                        } else {
                            Step::Index(self.compile_expression(index)?)
                        }
                    }
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
        Ok((place, fetch_line))
    }
}
