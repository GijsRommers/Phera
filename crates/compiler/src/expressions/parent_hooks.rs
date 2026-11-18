//! `parent::$name::get()` uses the statically named parent's property hook.
//! It keeps the ordinary argument sends and can only occur in the matching hook.

use phera_ast::{Arguments, Expression, ExpressionKind, MemberName, Variable};
use phera_bytecode::{ClassBinding, Reg};

use crate::{CompileError, Compiler, ir::Instruction};

impl Compiler<'_> {
    pub(super) fn compile_parent_hook(
        &mut self,
        target: &Expression,
        method: &MemberName,
        arguments: &Arguments,
        line: usize,
    ) -> Result<Option<Reg>, CompileError> {
        let ExpressionKind::StaticProperty {
            class,
            name: Variable::Named(property),
        } = &target.kind
        else {
            return Ok(None);
        };
        let MemberName::Identifier(method) = method else {
            return Ok(None);
        };
        let kind = method.name.to_ascii_lowercase();
        if !matches!(kind.as_str(), "get" | "set")
            || !matches!(&class.kind, ExpressionKind::Name(name) if name.text.eq_ignore_ascii_case("parent"))
            || self.source.get(target.span.start) == Some(&b'(')
        {
            return Ok(None);
        }
        if self.class_scope.is_none() {
            return Err(self.error("Cannot use \"parent\" when no class scope is active", line));
        }
        if matches!(arguments, Arguments::FirstClassCallable) {
            return Err(self.error("Cannot create Closure for parent property hook call", line));
        }
        let written = &method.name;
        let Some(active) = &self.hooked_property else {
            return Err(self.error(
                format!("Must not use parent::${property}::{written}() outside a property hook"),
                line,
            ));
        };
        if active != property {
            return Err(self.error(format!("Must not use parent::${property}::{written}() in a different property (${active})"), line));
        }
        if let Some((_, current)) = self.function.name.rsplit_once("::")
            && !current.eq_ignore_ascii_case(&kind)
        {
            return Err(self.error(format!("Must not use parent::${property}::{written}() in a different property hook ({})",current.to_ascii_lowercase()), line));
        }
        // A PHP method identifier cannot contain the leading dollar or double colon.
        self.emit(
            Instruction::KnownStaticCall {
                class: "parent".into(),
                method: format!("${property}::{kind}"),
                binding: ClassBinding::Forward,
            },
            line,
        );
        self.compile_arguments(arguments, line).map(Some)
    }
}
