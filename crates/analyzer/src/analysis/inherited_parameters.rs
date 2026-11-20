//! An override of `/** @param string $text */ function render($text)` keeps
//! the documented parameter type in its body, including after a rename.

use super::*;

impl Analyzer<'_> {
    pub(super) fn inherit_parameter_types(&self, method: &Method, scope: &mut Scope) {
        if !self.index.read_type_docs
            || method
                .parameters
                .iter()
                .all(|parameter| !needs_annotation(parameter, scope))
        {
            return;
        }
        let Some(class) = self.context.class.as_deref() else {
            return;
        };
        let key = method.name.name.to_ascii_lowercase();
        let own_doc = self
            .context
            .documentation_class
            .as_deref()
            .and_then(|class| self.index.class(class))
            .and_then(|class| class.methods.get(&key))
            .and_then(|method| method.documentation.as_deref());
        let receiver = ValueType::Class(class.into());
        for parent in self.index.ancestry(class).into_iter().skip(1) {
            let Some(signature) = parent.methods.get(&key) else {
                continue;
            };
            if signature.modifiers.visibility == Some(Visibility::Private) {
                continue;
            }
            let Some(doc) = &signature.documentation else {
                continue;
            };
            let typed = signature
                .clone()
                .typed(self.index)
                .for_receiver(self.index, parent, &receiver);
            for (parameter, inherited) in method.parameters.iter().zip(&typed.parameters) {
                if needs_annotation(parameter, scope)
                    && !own_doc.is_some_and(|doc| doc.params.contains_key(&parameter.name))
                    && doc.params.contains_key(&inherited.name)
                    && !inherited.ty.is_mixed()
                {
                    let ty = if parameter.default.as_ref().is_some_and(|default| {
                        matches!(default.kind, ExpressionKind::Literal(Literal::Null))
                    }) {
                        inherited.ty.join(&ValueType::Null)
                    } else {
                        inherited.ty.clone()
                    };
                    scope.define(&parameter.name, ty);
                }
            }
        }
    }
}

fn needs_annotation(parameter: &Parameter, scope: &Scope) -> bool {
    match scope.get(&parameter.name).ty {
        ValueType::ExplicitMixed => true,
        ValueType::Mixed | ValueType::ImplicitMixed => parameter.ty.is_none(),
        _ => false,
    }
}
