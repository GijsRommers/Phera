//! A local async context dispatches callback bodies through real standard natives.

use phera_ext_pcre::Pcre;
use phera_ext_standard::Standard;
use phera_extension_api::testing::{TestContext, oracle};
use phera_extension_api::{
    ArrayKey, Context, Diagnostics, Extension, LocalFuture, NativeError, NativeFn, NativeResult,
    PhpArray, Registry, RequestState, Value, prepare_arguments,
};
use std::future::Future;
use std::task::{Context as TaskContext, Poll, Waker};

pub struct Harness {
    pub base: TestContext,
    registry: Registry,
}
impl Default for Harness {
    fn default() -> Self {
        let mut registry = Registry::for_level(phera_extension_api::testing::oracle::test_level());
        Pcre.register(&mut registry);
        Standard.register(&mut registry);
        Self {
            base: TestContext::default(),
            registry,
        }
    }
}
impl Harness {
    pub fn run(&mut self, name: &str, arguments: Vec<Value>) -> NativeResult {
        let mut future = std::pin::pin!(self.invoke(name, arguments));
        let mut task = TaskContext::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut task) {
                Poll::Ready(result) => return result,
                Poll::Pending => continue,
            }
        }
    }
    fn invoke<'a>(
        &'a mut self,
        name: &'a str,
        mut arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            let entry = *self.registry.get_function(name).expect("registered native");
            prepare_arguments(&entry, &mut arguments, self)?;
            match entry.handler {
                NativeFn::Sync(function) => function(self, &arguments),
                NativeFn::Async(function) => function(self, arguments).await,
            }
        })
    }
}
impl Context for Harness {
    fn write(&mut self, bytes: &[u8]) {
        self.base.write(bytes);
    }
    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        &mut self.base.diagnostics
    }
    fn state(&mut self) -> &mut RequestState {
        &mut self.base.state
    }
    fn strict_types(&self) -> bool {
        false
    }
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            // Exercise a real await boundary on every callback.
            let mut yielded = false;
            std::future::poll_fn(|task| {
                if yielded {
                    Poll::Ready(())
                } else {
                    yielded = true;
                    task.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            let Value::String(name) = callable else {
                return Err(NativeError::type_error("Invalid test callable"));
            };
            let Value::Array(captures) = &arguments[0] else {
                panic!("expected match array");
            };
            let mut whole = captures.get(&ArrayKey::Int(0)).unwrap().clone();
            let name = name.to_string_lossy();
            match name.as_ref() {
                "identity" => return Ok(whole),
                "fail" => return Err(NativeError::value_error("callback failed")),
                "null_result" => return Ok(Value::Null),
                "array_result" => return Ok(Value::from(PhpArray::new())),
                "inspect" => return Ok(Value::from(oracle::encode(&arguments[0]))),
                _ => {}
            }
            if let Value::Array(pair) = whole {
                whole = pair.get(&ArrayKey::Int(0)).unwrap().clone();
            }
            let native = match name.as_ref() {
                "upper" => "strtoupper",
                "reverse" => "strrev",
                "length" => "strlen",
                other => panic!("unknown test callback {other}"),
            };
            self.invoke(native, vec![whole]).await
        })
    }
}
