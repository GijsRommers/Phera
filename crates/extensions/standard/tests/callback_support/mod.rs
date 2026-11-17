//! Local async/native callback harness; extension-api's TestContext rejects callbacks.

use std::future::Future;
use std::task::{Context as TaskContext, Poll, Waker};

use phera_ext_standard::Standard;
use phera_extension_api::testing::TestContext;
use phera_extension_api::{
    Context, Diagnostics, Extension, LocalFuture, NativeError, NativeFn, NativeResult, Registry,
    RequestState, Value, prepare_arguments,
};

pub struct Harness {
    pub base: TestContext,
    pub registry: Registry,
    pub trace: Vec<Value>,
    pub trace_enabled: bool,
    pub fixed: Option<Value>,
    pub throw_at: Option<usize>,
    pub calls: usize,
    pub target: Option<Value>,
    pub mutate: bool,
    pub replacement: Option<Value>,
    pub yield_callbacks: bool,
}

impl Default for Harness {
    fn default() -> Self {
        let mut registry = Registry::new();
        Standard.register(&mut registry);
        Self {
            base: TestContext::default(),
            registry,
            trace: Vec::new(),
            trace_enabled: true,
            fixed: None,
            throw_at: None,
            calls: 0,
            target: None,
            mutate: false,
            replacement: None,
            yield_callbacks: false,
        }
    }
}

pub fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::from(
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (phera_extension_api::ArrayKey::Int(index as i64), value))
            .collect::<phera_extension_api::PhpArray>(),
    )
}

pub fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = TaskContext::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("local test callbacks must complete without I/O"),
    }
}

impl Harness {
    pub fn invoke<'a>(
        &'a mut self,
        name: &'a str,
        mut arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            let entry = *self
                .registry
                .get_function(name)
                .ok_or_else(|| NativeError::error(format!("Unknown test function {name}")))?;
            prepare_arguments(&entry, &mut arguments, self)?;
            match entry.handler {
                NativeFn::Sync(handler) => handler(self, &arguments),
                NativeFn::Async(handler) => handler(self, arguments).await,
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
        self.base.strict_types
    }
    fn create_object(
        &mut self,
        class: &str,
        properties: phera_extension_api::PhpArray,
    ) -> NativeResult {
        self.base.create_object(class, properties)
    }

    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            if self.yield_callbacks {
                let mut yielded = false;
                std::future::poll_fn(|task| {
                    if yielded {
                        return Poll::Ready(());
                    }
                    yielded = true;
                    task.waker().wake_by_ref();
                    Poll::Pending
                })
                .await;
            }
            self.calls += 1;
            if self.trace_enabled {
                self.trace
                    .push(list(arguments.iter().map(|value| value.deref().clone())));
            }
            let instrumented = matches!(callable, Value::String(name)
                if name.as_bytes().starts_with(b"probe_"));
            if instrumented && self.throw_at == Some(self.calls) {
                return Err(NativeError::value_error("stop"));
            }
            if self.mutate
                && let Some(Value::Reference(target)) = &self.target
            {
                *target.borrow_mut() = self
                    .replacement
                    .clone()
                    .unwrap_or_else(|| list([Value::Int(99)]));
            }
            if let Some(value) = &self.fixed {
                return Ok(value.clone());
            }
            let Value::String(name) = callable else {
                return Err(NativeError::type_error("Invalid test callback"));
            };
            let name = name.to_string_lossy();
            match name.as_ref() {
                "probe_string" => self.invoke("strcmp", arguments).await,
                "probe_natural" => self.invoke("strnatcmp", arguments).await,
                "probe_append" | "probe_delete" => {
                    if let Some(Value::Reference(target)) = &self.target {
                        let mut target = target.borrow_mut();
                        if let Value::Array(array) = &mut *target {
                            let array = std::rc::Rc::make_mut(array);
                            if name == "probe_append" {
                                array.push(Value::Int(1))?;
                            } else {
                                array.remove(&phera_extension_api::ArrayKey::Int(1));
                            }
                        }
                    }
                    Ok(Value::Null)
                }
                "probe_increment" | "probe_increment_second" => {
                    let index = usize::from(name == "probe_increment_second");
                    let argument = &arguments[index];
                    let number = phera_extension_api::value::to_int(argument, self.diagnostics())?;
                    if let Value::Reference(cell) = argument {
                        *cell.borrow_mut() = Value::Int(number + 1);
                    } else {
                        self.warning(format!(
                            "{name}(): Argument #{} ($v) must be passed by reference, value given",
                            index + 1
                        ));
                    }
                    Ok(Value::Int(number + 1))
                }
                "probe_walk" => {
                    let Some(Value::Reference(element)) = arguments.first() else {
                        return Err(NativeError::error("walk callback needs reference"));
                    };
                    let extra = arguments.get(2).cloned().unwrap_or(Value::Null);
                    let original = element.borrow().clone();
                    *element.borrow_mut() = list([original, arguments[1].clone(), extra]);
                    Ok(Value::Null)
                }
                "probe_truth" => Ok(Value::Bool(phera_extension_api::value::cast_to_bool(
                    &arguments[0],
                    self.diagnostics(),
                ))),
                "probe_compare" => {
                    let left =
                        phera_extension_api::value::to_int(&arguments[0], self.diagnostics())?;
                    let right =
                        phera_extension_api::value::to_int(&arguments[1], self.diagnostics())?;
                    Ok(Value::Int(match left.cmp(&right) {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Equal => 0,
                        std::cmp::Ordering::Greater => 1,
                    }))
                }
                "is_int" => Ok(Value::Bool(arguments[0].deref().is_int())),
                _ => self.invoke(&name, arguments).await,
            }
        })
    }
}

pub const PHP_CALLBACKS: &str = r#"
$REPLACEMENT = [99];
$TRACE = []; $FIXED = null; $HAS_FIXED = false; $THROW_AT = null; $CALLS = 0; $MUTATE = false;
function record_call($args) {
    global $TRACE, $CALLS, $THROW_AT, $MUTATE, $TARGET;
    $TRACE[] = $args;
    if (++$CALLS === $THROW_AT) throw new ValueError('stop');
    if ($MUTATE) $TARGET = $GLOBALS['REPLACEMENT'];
}
function probe_delete(&$v, $k) { record_call(func_get_args()); unset($GLOBALS['TARGET'][1]); }
function probe_increment(&$v, $k = null) {
    record_call(func_get_args()); return ++$v;
}
function probe_increment_second($carry, &$v) {
    record_call(func_get_args()); return ++$v;
}
function probe_string($a, $b) { record_call([$a,$b]); return strcmp($a,$b); }
function probe_natural($a, $b) { record_call([$a,$b]); return strnatcmp($a,$b); }
function probe_compare($a, $b) {
    record_call([$a, $b]);
    return $GLOBALS['HAS_FIXED'] ? $GLOBALS['FIXED'] : (int)$a <=> (int)$b;
}
function probe_truth($v, $k = null) { record_call(func_get_args()); return (bool)$v; }
function probe_walk(&$v, $k, $arg = null) {
    record_call(func_get_args()); $v = [$v, $k, $arg];
}
"#;
