//! Drain one writer buffer to memory, a URI, or a caller-owned stream.
//!
//! ```php
//! $writer = XMLWriter::toStream($stream);
//! $writer->writeElement('root');
//! $bytesWritten = $writer->flush();
//! ```

use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Registry, Value,
    value::{Fork, Resource},
};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::writer::Writer;

pub const BUFFER_SIZE: usize = 4096;

#[derive(Clone)]
pub enum Target {
    Memory,
    Uri(Rc<Mutex<tokio::fs::File>>),
    Stream(Rc<Resource>),
}

pub struct State {
    pub writer: Writer,
    target: Target,
    memory: Vec<u8>,
    failed: bool,
    closing: Option<Closing>,
}

pub type Shared = Rc<RefCell<State>>;

type Closing = Rc<RefCell<Vec<(Target, Vec<u8>)>>>;

#[derive(Default)]
struct PendingOutputs {
    live: Vec<Weak<RefCell<State>>>,
    dropped: Closing,
    registered: bool,
}

pub fn register_request_state(registry: &mut Registry) {
    registry.fork_state::<PendingOutputs>(phera_extension_api::fresh_state);
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(closing) = &self.closing
            && !self.failed
            && !self.writer.output.is_empty()
        {
            closing
                .borrow_mut()
                .push((self.target.clone(), self.writer.take_output()));
        }
    }
}

pub fn install(context: &mut dyn Context, object: &Object, target: Target) {
    let pending = !matches!(target, Target::Memory);
    let closing = pending.then(|| {
        context
            .state()
            .get_or_default::<PendingOutputs>()
            .dropped
            .clone()
    });
    let shared = Rc::new(RefCell::new(State {
        writer: Writer::new(),
        target,
        memory: Vec::new(),
        failed: false,
        closing,
    }));
    *object.native.borrow_mut() = Some(Box::new(shared.clone()));
    if pending {
        object.expect_destructor();
        let outputs = context.state().get_or_default::<PendingOutputs>();
        if outputs.live.len() == outputs.live.capacity() {
            outputs.live.retain(|writer| writer.strong_count() > 0);
        }
        outputs.live.push(Rc::downgrade(&shared));
        if !outputs.registered {
            outputs.registered = true;
            context.register_request_finalizer(finish_pending);
        }
    }
}

pub fn release<'a>(context: &'a mut dyn Context, object: &'a Object) -> LocalFuture<'a, ()> {
    Box::pin(async move {
        let writer = object
            .native
            .borrow_mut()
            .take()
            .and_then(|native| native.downcast::<Shared>().ok());
        if let Some(writer) = writer {
            publish(context, &writer, "XMLWriter").await;
        }
    })
}

fn finish_pending(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let pending = context.state().get_or_default::<PendingOutputs>();
        let dropped = std::mem::take(&mut *pending.dropped.borrow_mut());
        let live = std::mem::take(&mut pending.live);
        for (target, data) in dropped {
            write_target(context, target, &data, "XMLWriter").await;
        }
        for output in live.into_iter().filter_map(|weak| weak.upgrade()) {
            publish(context, &output, "XMLWriter").await;
        }
        Ok(Value::Null)
    })
}

pub fn state(object: &Object) -> Result<Shared, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Shared>())
        .cloned()
        .ok_or_else(uninitialized)
}

pub fn uninitialized() -> NativeError {
    NativeError::error("Invalid or uninitialized XMLWriter object")
}

pub fn fork(_: &Object, native: &dyn Any, _: &mut Fork<'_>) -> Result<Box<dyn Any>, String> {
    let writer = native
        .downcast_ref::<Shared>()
        .ok_or("its XML writer is not initialized")?;
    let source = writer.borrow();
    if !matches!(source.target, Target::Memory) {
        return Err(
            "its output file or stream belongs to one request; open it after Phera\\snapshot()"
                .into(),
        );
    }
    Ok(Box::new(Rc::new(RefCell::new(State {
        writer: source.writer.clone(),
        target: Target::Memory,
        memory: source.memory.clone(),
        failed: source.failed,
        closing: None,
    }))))
}

pub async fn publish(context: &mut dyn Context, state: &Shared, function: &str) -> i64 {
    let (target, output) = {
        let mut state = state.borrow_mut();
        if state.failed {
            return -1;
        }
        if state.writer.output.is_empty() {
            return 0;
        }
        let data = state.writer.take_output();
        if matches!(state.target, Target::Memory) {
            let length = data.len() as i64;
            state.memory.extend(data);
            return length;
        }
        (state.target.clone(), data)
    };
    let result = write_target(context, target, &output, function).await;
    if result < 0 {
        state.borrow_mut().failed = true;
    }
    result
}

async fn write_target(
    context: &mut dyn Context,
    target: Target,
    output: &[u8],
    function: &str,
) -> i64 {
    match target {
        Target::Uri(file) => {
            let mut file = file.lock().await;
            match file.write_all(output).await {
                Ok(()) if file.flush().await.is_ok() => output.len() as i64,
                _ => -1,
            }
        }
        Target::Stream(stream) if !stream.is_closed() => {
            let length = output.len() as i64;
            let result =
                phera_ext_streams::native::write_resource(context, &stream, output, function).await;
            match result {
                Ok(Value::Int(written)) if written == length => written,
                _ => -1,
            }
        }
        Target::Stream(_) | Target::Memory => -1,
    }
}

pub async fn flush(
    context: &mut dyn Context,
    state: &Shared,
    function: &str,
    memory_only: bool,
    empty: bool,
) -> NativeResult {
    let memory = matches!(state.borrow().target, Target::Memory);
    if memory_only && !memory {
        return Ok(Value::from(""));
    }
    let count = publish(context, state, function).await;
    if memory {
        let mut state = state.borrow_mut();
        let output = if empty {
            std::mem::take(&mut state.memory)
        } else {
            state.memory.clone()
        };
        Ok(Value::from(output))
    } else {
        Ok(Value::Int(count))
    }
}
