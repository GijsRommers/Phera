//! A native operation can retain its output cursor while PHP properties are read.

use std::rc::Rc;

use crate::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PropertyPurpose, Unwind,
    Value,
};

/// Owned native state waiting for a declared property's view.
pub trait PropertyViewContinuation {
    fn resume(
        self: Box<Self>,
        context: &mut dyn Context,
        properties: Rc<PhpArray>,
    ) -> LocalFuture<'_, NativeResult>;

    /// A getter threw before a snapshot could be produced.
    fn abandon(self: Box<Self>, _context: &mut dyn Context) {}
}

pub struct PropertyViewRequest {
    pub object: Rc<Object>,
    pub purpose: PropertyPurpose,
    pub function: &'static str,
    pub arguments: Vec<Value>,
    pub continuation: Box<dyn PropertyViewContinuation>,
}

impl PropertyViewRequest {
    /// Hand retained state to the runtime at a supported native call boundary.
    pub fn suspend(self) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(self)))
    }

    pub fn take(error: NativeError) -> Result<Self, NativeError> {
        let NativeError::Unwind(payload) = error else {
            return Err(error);
        };
        payload
            .0
            .downcast::<Self>()
            .map(|request| *request)
            .map_err(|payload| NativeError::Unwind(Unwind(payload)))
    }
}
