//! `forward_static_call([ParentClass::class, 'who'])` preserves the late-static class.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

const CALLBACK: Param = Param::new("callback", ParamType::Callable);

pub(super) fn register(registry: &mut Registry) {
    registry.function_with_extra_named_handler(
        FunctionEntry::asynchronous(
            "forward_static_call",
            const { &[CALLBACK, Param::new("args", ParamType::Mixed).variadic()] },
            call,
        ),
        |_, _| {
            Err(NativeError::argument_count_error(
                "forward_static_call() does not accept unknown named parameters",
            ))
        },
    );
    registry.function(FunctionEntry::asynchronous(
        "forward_static_call_array",
        const { &[CALLBACK, Param::new("args", ParamType::Array)] },
        call_array,
    ));
}

fn call(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut arguments = arguments.into_iter();
        let callback = arguments.next().unwrap_or_default();
        let count = arguments.len();
        let parameters = PhpArray::from_values(arguments, count);
        context
            .forward_static_callback("forward_static_call", &callback, &parameters)
            .await
    })
}

fn call_array(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let parameters = args::array(&arguments, 1).expect("native signature requires an array");
        let callback = args::value(&arguments, 0);
        context
            .forward_static_callback("forward_static_call_array", callback, parameters)
            .await
    })
}
