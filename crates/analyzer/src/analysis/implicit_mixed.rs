//! At level 10, `requestData()['name']` is mixed if requestData declares
//! a bare array return type. Inference failures keep their unknown origin.

use super::*;
use crate::{
    index::{ClassInfo, Index, Signature},
    types::{Collection, CollectionKind, Compatibility, TypePolicy},
};
use std::sync::Arc;

impl Signature {
    pub(super) fn with_inherited_array_return(
        mut self,
        index: &Index,
        owner: &ClassInfo,
        method: &str,
        receiver: &ValueType,
    ) -> Self {
        if self.returns != ValueType::Array
            || !index.read_type_docs
            || self.modifiers.visibility == Some(Visibility::Private)
            || self
                .documentation
                .as_ref()
                .is_some_and(|doc| doc.returns.is_some())
        {
            return self;
        }
        let key = method.to_ascii_lowercase();
        for ancestor in index.ancestry(&owner.name).into_iter().skip(1) {
            if ancestor.kind == ClassKind::Trait {
                continue;
            }
            let Some(inherited) = ancestor.methods.get(&key) else {
                continue;
            };
            if inherited.modifiers.visibility == Some(Visibility::Private)
                || inherited
                    .documentation
                    .as_ref()
                    .is_none_or(|doc| doc.returns.is_none())
            {
                continue;
            }
            let returns = inherited
                .clone()
                .typed(index)
                .for_receiver(index, ancestor, receiver)
                .returns;
            if matches!(returns, ValueType::Shape(_) | ValueType::Collection(_))
                && self
                    .returns
                    .accepts(&returns, index, TypePolicy::Documentation)
                    == Compatibility::Accepts
            {
                self.returns = returns;
            }
            break;
        }
        self
    }
}

impl Analyzer<'_> {
    pub(super) fn untyped_callable_result(&self, callable: &ValueType) -> ValueType {
        if self.level < 10 {
            return ValueType::Mixed;
        }
        let arms = match callable {
            ValueType::Union(arms) => arms.as_ref(),
            callable => std::slice::from_ref(callable),
        };
        let mut result = ValueType::Never;
        for arm in arms {
            let returns = match arm {
                ValueType::UnknownCallable => ValueType::ImplicitMixed,
                ValueType::Class(class) if class.eq_ignore_ascii_case("Closure") => {
                    ValueType::ImplicitMixed
                }
                ValueType::Callable(signature) => signature.returns.clone(),
                _ => return ValueType::Mixed,
            };
            result = result.join(&returns);
        }
        result
    }

    pub(super) fn declared_array_result(
        &self,
        signature: &Signature,
        result: ValueType,
    ) -> ValueType {
        if self.level < 10
            || result != ValueType::Array
            || signature.returns != ValueType::Array
            || signature.location.file.starts_with("ext/")
            || signature.location.file.starts_with("Zend/")
            || signature
                .documentation
                .as_ref()
                .and_then(|doc| doc.returns.as_ref())
                .is_some_and(|annotation| annotation.ty != ValueType::Array)
        {
            return result;
        }
        ValueType::Collection(Arc::new(Collection {
            kind: CollectionKind::Array,
            key: ValueType::Int.join(&ValueType::String).benevolent(),
            value: ValueType::ImplicitMixed,
            non_empty: false,
        }))
    }
}
