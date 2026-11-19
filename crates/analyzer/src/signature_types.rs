//! A call to `/** @return User */ function load()` has result type User.
//! The declaration retains its native contract for validation at the source location.

use crate::{
    index::{Index, PropertyInfo, Signature},
    types::{Compatibility, TypePolicy, ValueType},
};

impl Signature {
    pub fn typed(mut self, index: &Index) -> Self {
        if let Some(doc) = &self.documentation {
            for parameter in &mut self.parameters {
                if let Some(annotation) = doc.params.get(&parameter.name) {
                    parameter.ty = refine(&parameter.ty, &annotation.ty, index);
                }
            }
            if let Some(annotation) = &doc.returns {
                self.returns = refine(&self.returns, &annotation.ty, index);
            }
        }
        self
    }
}

impl PropertyInfo {
    pub fn effective_type(&self, index: &Index) -> ValueType {
        self.documented_type.as_ref().map_or_else(
            || self.ty.clone(),
            |annotation| refine(&self.ty, annotation, index),
        )
    }
}

fn refine(native: &ValueType, documented: &ValueType, index: &Index) -> ValueType {
    if native.accepts(documented, index, TypePolicy::Documentation) == Compatibility::Accepts {
        documented.clone()
    } else {
        native.clone()
    }
}
