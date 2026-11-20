//! An implementation of `/** @return Item|null */ function find()`
//! still needs a return statement when the PHPDoc fallthrough check is enabled.

use super::*;
use crate::types::CollectionKind;

impl Analyzer<'_> {
    pub(super) fn fallthrough_doc_type(&self) -> Option<ValueType> {
        if self.context.declared_return {
            return None;
        }
        let check_nullable = self
            .options
            .check_phpdoc_missing_return
            .unwrap_or(self.level >= 2);
        let documented = self.context.documented_return.clone().or_else(|| {
            if check_nullable {
                self.inherited_return_type()
            } else {
                None
            }
        });
        documented.filter(|ty| {
            (check_nullable || self.level >= 3 && ty.excludes_null())
                && !super::value_contracts::allows_empty_return(ty)
        })
    }

    fn inherited_return_type(&self) -> Option<ValueType> {
        let class = self.context.class.as_ref()?;
        let name = self.context.function.to_ascii_lowercase();
        let receiver = ValueType::Class(class.clone());
        for parent in self.index.ancestry(class).into_iter().skip(1) {
            if let Some(signature) = parent.methods.get(&name)
                && signature.modifiers.visibility != Some(Visibility::Private)
                && signature
                    .documentation
                    .as_ref()
                    .is_some_and(|doc| doc.returns.is_some())
            {
                return Some(
                    signature
                        .clone()
                        .typed(self.index)
                        .for_receiver(self.index, parent, &receiver)
                        .returns,
                );
            }
        }
        None
    }

    pub(super) fn report_doc_fallthrough(&mut self, ty: &ValueType, span: Span) {
        let name = &self.context.function;
        let callable = if name == "Closure" {
            "Anonymous function".to_owned()
        } else if let Some(class) = &self.context.class {
            format!("Method {class}::{name}()")
        } else {
            format!("Function {name}()")
        };
        let ty = type_label(ty);
        self.diagnose(
            "return.missing",
            span,
            format!("{callable} should return {ty} but return statement is missing."),
        );
    }
}

fn type_label(ty: &ValueType) -> String {
    let arms = match ty {
        ValueType::Union(arms) => arms.as_ref(),
        ty => std::slice::from_ref(ty),
    };
    let mut names: Vec<_> = arms
        .iter()
        .map(|arm| (super::member_checks::type_order(arm), member_name(arm)))
        .collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>()
        .join("|")
}

fn member_name(ty: &ValueType) -> String {
    if let ValueType::Collection(collection) = ty
        && collection.kind == CollectionKind::Array
        && let ValueType::Union(keys) = &collection.key
        && keys.len() == 2
        && keys.contains(&ValueType::Int)
        && keys.contains(&ValueType::String)
    {
        let prefix = if collection.non_empty {
            "non-empty-"
        } else {
            ""
        };
        return format!("{prefix}array<{}>", collection.value);
    }
    ty.to_string()
}
