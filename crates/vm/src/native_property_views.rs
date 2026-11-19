//! `json_encode($row)` can wait for a property hook while retaining its
//! encoder state outside the Rust callback stack.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{
    NativeError, NativeResult, PropertyPurpose, PropertyViewContinuation, PropertyViewRequest,
};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;
use crate::access_mode::Binding;
use crate::error_traces::NativeTrace;
use crate::hooked_property_iteration::{HookedPropertyIterator, IterationStep};
use crate::property_hook_continuation::HookContinuation;

pub(crate) struct NativeView {
    cursor: HookedPropertyIterator,
    properties: PhpArray,
    purpose: PropertyPurpose,
    destination: Reg,
    pub(crate) native: NativeTrace,
    pub(crate) continuation: Box<dyn PropertyViewContinuation>,
}

impl Vm {
    pub(crate) async fn complete_native_view_result(
        &mut self,
        destination: Reg,
        result: NativeResult,
    ) -> Result<Option<Value>, NativeError> {
        let request = match result {
            Ok(value) => return Ok(Some(value)),
            Err(error) => PropertyViewRequest::take(error)?,
        };
        let view = self.native_view_request(destination, request)?;
        self.drive_native_view(view).await
    }

    pub(crate) async fn resume_native_view(
        &mut self,
        mut view: Box<NativeView>,
        key: ArrayKey,
        value: Value,
    ) -> Result<(), NativeError> {
        let destination = view.destination;
        view.properties.insert(key, value);
        if let Some(value) = self.drive_native_view(*view).await? {
            self.set(destination, value);
        }
        Ok(())
    }

    fn native_view_request(
        &self,
        destination: Reg,
        request: PropertyViewRequest,
    ) -> Result<NativeView, NativeError> {
        let class = self
            .hooked_property_class(&request.object)
            .ok_or_else(|| internal_error!("native property view class"))?;
        Ok(NativeView {
            cursor: HookedPropertyIterator::new(request.object, class, Binding::ByValue),
            properties: PhpArray::new(),
            purpose: request.purpose,
            destination,
            native: NativeTrace::new(request.function, &request.arguments),
            continuation: request.continuation,
        })
    }

    async fn drive_native_view(
        &mut self,
        mut view: NativeView,
    ) -> Result<Option<Value>, NativeError> {
        loop {
            match view.cursor.next(self, view.purpose) {
                Err(error) => {
                    view.continuation.abandon(self);
                    return Err(error);
                }
                Ok(IterationStep::Entry(key, value)) => {
                    view.properties.insert(key, value);
                }
                Ok(IterationStep::Getter(function, key, _)) => {
                    let object = view.cursor.object.clone();
                    self.suspend_for_hook(
                        function,
                        object,
                        None,
                        HookContinuation::NativeView(Box::new(view), key),
                    )
                    .await?;
                    return Ok(None);
                }
                Ok(IterationStep::End) => {
                    let depth = self.call_stack.depth();
                    let previous = self.native_view_depth.replace(depth);
                    let result = view
                        .continuation
                        .resume(self, Rc::new(view.properties))
                        .await;
                    self.native_view_depth = previous;
                    view = match result {
                        Ok(value) => return Ok(Some(value)),
                        Err(error) => self.native_view_request(
                            view.destination,
                            PropertyViewRequest::take(error)?,
                        )?,
                    };
                }
            }
        }
    }

    pub(crate) fn abandon_native_view(&mut self) {
        if self
            .call_stack
            .current()
            .is_some_and(|frame| frame.destination().is_none() && frame.has_extras())
            && let Some(continuation) = self.call_stack.current_extras_mut().property_hook.take()
            && let HookContinuation::NativeView(view, _) = *continuation
        {
            view.continuation.abandon(self);
        }
    }
}
