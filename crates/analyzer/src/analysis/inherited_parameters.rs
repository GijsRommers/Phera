//! An override of `/** @param string $text */ function render($text)` keeps
//! the documented parameter type in its body, including after a rename.

use super::*;
impl Analyzer<'_> {
    pub(super) fn inherit_parameter_types(&self, method: &Method, scope: &mut Scope) {
        if self.level < 10
            || method
                .parameters
                .iter()
                .all(|parameter| scope.get(&parameter.name).ty != ValueType::ImplicitMixed)
        {
            return;
        }
        let Some(class) = self.context.class.as_deref() else {
            return;
        };
        let key = method.name.name.to_ascii_lowercase();
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
                if scope.get(&parameter.name).ty == ValueType::ImplicitMixed
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
