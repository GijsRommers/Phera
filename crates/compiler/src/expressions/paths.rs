//! Evaluate variable paths while preserving the separate PHP global namespace.

use crate::ir::{Constant, Place, PlaceScope};
use crate::{CompileError, Compiler, writable};
use phera_ast as ast;

impl Compiler<'_> {
    /// Walk indexed targets iteratively, then evaluate index expressions from root to leaf.
    pub(crate) fn place(&mut self, error: &ast::Expression) -> Result<Place, CompileError> {
        let line = self.lines.line(error.span.start);
        let mut node = error;
        let mut path = vec![];
        while let ast::ExpressionKind::Index { target, index } = &node.kind {
            path.push(index.as_deref());
            node = target;
        }
        let mut place = self.path_root(node, line)?;
        let mut steps = vec![];
        for index in path.into_iter().rev() {
            steps.push(
                index
                    .map(|error| self.compile_expression(error))
                    .transpose()?,
            )
        }
        if place.scope == PlaceScope::Globals && !steps.is_empty() {
            let scratch = place.base;
            place.base = steps
                .remove(0)
                .ok_or_else(|| self.error("Cannot use [] for reading", line))?;
            place.scope = PlaceScope::Global(scratch);
        }
        place.path = steps;
        Ok(place)
    }

    /// Allow a computed root for reads while preserving iterative postfix traversal.
    pub(crate) fn readable_place(
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
        let base = self.compile_expression(node)?;
        let mut path = vec![];
        for index in indices.into_iter().rev() {
            path.push(
                index
                    .map(|error| self.compile_expression(error))
                    .transpose()?,
            )
        }
        Ok(Place {
            base,
            scope: PlaceScope::Local,
            path,
        })
    }

    /// Superglobal roots bypass the local namespace without a function-entry binding.
    fn path_root(&mut self, node: &ast::Expression, line: usize) -> Result<Place, CompileError> {
        let (base, scope) = match &node.kind {
            ast::ExpressionKind::Variable(ast::Variable::Named(index)) => {
                if index == "this" {
                    return Err(self.error("Cannot re-assign $this", line));
                }
                if index == "GLOBALS" {
                    (self.allocate_register()?, PlaceScope::Globals)
                } else if matches!(
                    index.as_str(),
                    "_GET"
                        | "_POST"
                        | "_COOKIE"
                        | "_SERVER"
                        | "_REQUEST"
                        | "_FILES"
                        | "_ENV"
                        | "_SESSION"
                ) {
                    (
                        self.literal(Constant::Bytes(index.as_bytes().to_vec()), line)?,
                        PlaceScope::Global(self.allocate_register()?),
                    )
                } else {
                    (self.local(index)?, PlaceScope::Local)
                }
            }
            ast::ExpressionKind::Variable(ast::Variable::Dynamic(error)) => {
                self.function.dynamic_scope = true;
                (self.compile_expression(error)?, PlaceScope::Dynamic)
            }
            _ => return Err(self.error("Expression is not a writable place", line)),
        };
        Ok(Place {
            base,
            scope,
            path: vec![],
        })
    }
}
