//! `$request?->query()->all()` skips both calls when the request is null.
//! The skipped path remains distinct from null returned by an evaluated call.

use super::*;

pub(super) struct ChainValue {
    pub ty: ValueType,
    skipped: Option<Scope>,
}

impl ChainValue {
    pub fn plain(ty: ValueType) -> Self {
        Self { ty, skipped: None }
    }

    pub fn skip_null(&mut self, scope: &Scope) {
        if !self.ty.excludes_null() {
            self.skipped = Some(match self.skipped.take() {
                Some(path) => Scope::join(&[path, scope.clone()]),
                None => scope.clone(),
            });
            self.ty = self.ty.without_null();
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
