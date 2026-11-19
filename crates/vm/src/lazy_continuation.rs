//! A suspended `$record->name` resumes with its already evaluated property name.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, Unwind};
use phera_value::{Object, Value};

use crate::Vm;
use crate::error_traces::NativeTrace;
use crate::magic_mutations::MutationPath;
use crate::magic_places::PathRead;
use crate::property_hook_continuation::{ReadCompletion, WriteCompletion};

pub(crate) enum LazyContinuation {
    Read(PathRead, ReadCompletion),
    Mutation(MutationPath, WriteCompletion),
    Reflection(Reg, NativeTrace),
}

impl LazyContinuation {
    pub(crate) fn native_trace(&self) -> Option<&NativeTrace> {
        match self {
            Self::Reflection(_, trace) => Some(trace),
            _ => None,
        }
    }
}

pub(crate) struct ReflectInitialize(pub(crate) Rc<Object>);

impl ReflectInitialize {
    pub(crate) fn suspend(self) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(self)))
    }

    pub(crate) fn take(error: NativeError) -> Result<Self, NativeError> {
        let NativeError::Unwind(payload) = error else {
            return Err(error);
        };
        payload
            .0
            .downcast::<Self>()
            .map(|call| *call)
            .map_err(|payload| NativeError::Unwind(Unwind(payload)))
    }
}

impl Vm {
    pub(crate) async fn finish_reflected_lazy(
        &mut self,
        request: ReflectInitialize,
        destination: Option<Reg>,
    ) -> Result<Option<Value>, NativeError> {
        let ReflectInitialize(object) = request;
        let Some(destination) = destination else {
            let (initialization, callback) = self.begin_lazy_initialization(object.clone())?;
            let callee = self.resolve_callable(&callback)?;
            let value = self
                .invoke_resolved_callable(callee, [Value::Object(object)])
                .await?;
            return initialization.finish(value).map(Some);
        };
        let trace = NativeTrace::new(
            "ReflectionClass->initializeLazyObject",
            &[Value::Object(object.clone())],
        );
        let continuation = LazyContinuation::Reflection(destination, trace);
        if let Some(returned) = self
            .suspend_lazy_initialization(object, continuation)
            .await?
        {
            self.resume_property_hook(returned).await?;
        }
        Ok(None)
    }
}
