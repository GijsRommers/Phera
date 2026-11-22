//! `json_encode($row)` can wait for an initializer or property hook while
//! retaining its encoder state outside the Rust callback stack.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{
    Context, NativeError, NativeResult, PropertyPurpose, PropertyViewContinuation,
    PropertyViewRequest,
};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;
use crate::access_mode::Binding;
use crate::error_traces::NativeTrace;
use crate::hooked_property_iteration::{HookedPropertyIterator, IterationStep};
use crate::lazy_continuation::LazyContinuation;
use crate::lazy_objects::resolved_lazy_instance;
use crate::property_hook_continuation::HookContinuation;

pub(crate) struct NativeView {
    cursor: HookedPropertyIterator,
    properties: PhpArray,
    purpose: PropertyPurpose,
    destination: Reg,
    pub(crate) native: NativeTrace,
    pub(crate) continuation: Box<dyn PropertyViewContinuation>,
}

enum ViewWork {
    Request(PropertyViewRequest),
    Enumerate(NativeView),
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
        self.drive_view_request(destination, request).await
    }

    pub(crate) async fn resume_native_view(
        &mut self,
        mut view: Box<NativeView>,
        key: ArrayKey,
        value: Value,
    ) -> Result<(), NativeError> {
        let destination = view.destination;
        view.properties.insert(key, value);
        if let Some(value) = self
            .drive_native_views(destination, ViewWork::Enumerate(*view))
            .await?
        {
            self.set_call_result(destination, value);
        }
        Ok(())
    }

    pub(crate) async fn drive_view_request(
        &mut self,
        destination: Reg,
        request: PropertyViewRequest,
    ) -> Result<Option<Value>, NativeError> {
        self.drive_native_views(destination, ViewWork::Request(request))
            .await
    }

    async fn drive_native_views(
        &mut self,
        destination: Reg,
        mut work: ViewWork,
    ) -> Result<Option<Value>, NativeError> {
        loop {
            let (continuation, properties) = match work {
                ViewWork::Request(request) => {
                    let object = resolved_lazy_instance(request.object.clone());
                    if object.is_uninitialized_lazy() {
                        let trace = NativeTrace::new(request.function, &request.arguments);
                        let continuation =
                            LazyContinuation::NativeView(destination, Box::new(request), trace);
                        self.suspend_lazy_initialization(object, continuation)
                            .await?;
                        return Ok(None);
                    }
                    if let Some(class) = self.hooked_property_class(&object)
                        && request.purpose != PropertyPurpose::Serialize
                    {
                        work = ViewWork::Enumerate(NativeView {
                            cursor: HookedPropertyIterator::new(object, class, Binding::ByValue),
                            properties: PhpArray::new(),
                            purpose: request.purpose,
                            destination,
                            native: NativeTrace::new(request.function, &request.arguments),
                            continuation: request.continuation,
                        });
                        continue;
                    }
                    let properties = match request.purpose {
                        PropertyPurpose::Serialize => {
                            self.ordered_serialization_properties(&object)
                        }
                        _ => self.object_properties(&object, request.purpose),
                    };
                    match properties {
                        Ok(properties) => (request.continuation, properties),
                        Err(error) => {
                            request.continuation.abandon(self);
                            return Err(error);
                        }
                    }
                }
                ViewWork::Enumerate(mut view) => loop {
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
                        Ok(IterationStep::End) => break (view.continuation, view.properties),
                    }
                },
            };
            let previous = self.native_view_depth.replace(self.call_stack.depth());
            let result = continuation.resume(self, Rc::new(properties)).await;
            self.native_view_depth = previous;
            match result {
                Ok(value) => return Ok(Some(value)),
                Err(error) => work = ViewWork::Request(PropertyViewRequest::take(error)?),
            }
        }
    }

    pub(crate) fn abandon_native_view(&mut self) {
        if self
            .call_stack
            .current()
            .is_some_and(|frame| frame.destination().is_none() && frame.has_extras())
            && let Some(continuation) = self.call_stack.current_extras_mut().property_hook.take()
        {
            match *continuation {
                HookContinuation::NativeView(view, _) => view.continuation.abandon(self),
                HookContinuation::Lazy(_, LazyContinuation::NativeView(_, request, _)) => {
                    request.continuation.abandon(self);
                }
                _ => {}
            }
        }
    }
}
