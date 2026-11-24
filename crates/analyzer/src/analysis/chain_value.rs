//! `$request?->query()->all()` skips both calls when the request is null.
//! The skipped path remains distinct from null returned by an evaluated call.
//! In `$node?->copy($node->name())`, the argument runs with a non-null receiver.

use super::*;

pub(super) struct ChainValue {
    pub ty: ValueType,
    skipped: Option<Scope>,
}

impl ChainValue {
    pub fn plain(ty: ValueType) -> Self {
        Self { ty, skipped: None }
    }

    pub fn skip_null(&mut self, target: &Expression, scope: &mut Scope) {
        if !self.ty.excludes_null() {
            self.skipped = Some(match self.skipped.take() {
                Some(path) => Scope::join(&[path, scope.clone()]),
                None => scope.clone(),
            });
            let non_null = self.ty.without_null();
            if non_null != self.ty
                && non_null != ValueType::Never
                && let Some(place) = crate::scope::Place::of(target)
            {
                scope.facts.record(place, non_null.clone(), None);
            }
            self.ty = non_null;
        }
    }

    pub fn finish(self, scope: &mut Scope) -> ValueType {
        let Some(skipped) = self.skipped else {
            return self.ty;
        };
        *scope = if self.ty == ValueType::Never {
            skipped
        } else {
            Scope::join(&[std::mem::take(scope), skipped])
        };
        self.ty.join(&ValueType::Null)
    }
}
