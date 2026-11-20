//! Read and write filters retain their own PHP object while the stream stays open.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    io,
    rc::{Rc, Weak},
};

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value,
    value::{Resource, to_int},
};

use crate::{request::new_resource, user_file::invoke};

use super::super::{MAX_OUTPUT, MAX_STAGES, bounded};
use super::{FEED, PASS, buckets};

#[derive(Default)]
struct Streams(HashMap<i64, Rc<Chains>>);

pub(super) fn register_fork(registry: &mut phera_extension_api::Registry) {
    registry.fork_state::<Streams>(|state, fork| {
        let streams = state.downcast_ref::<Streams>()?;
        for chains in streams.0.values() {
            let read = chains.read.borrow();
            let write = chains.write.borrow();
            if let Some(stage) = read.first().or_else(|| write.first()) {
                fork.refuse_state(
                    stage.filter.class.name(),
                    "an attached user stream filter belongs to the boot",
                );
                break;
            }
            if chains.has_buffered() {
                fork.refuse_state("stream", "unread filtered bytes belong to the boot");
                break;
            }
        }
        None
    });
}

#[derive(Default)]
pub(crate) struct Chains {
    read: RefCell<Vec<Stage>>,
    write: RefCell<Vec<Stage>>,
    busy: Cell<bool>,
    exhausted: Cell<bool>,
    buffered: RefCell<VecDeque<u8>>,
    produced: Cell<usize>,
    stream: RefCell<Weak<Resource>>,
}

#[derive(Clone)]
struct Stage {
    id: i64,
    filter: Rc<Object>,
}

struct Attachment(i64);

pub(crate) fn lookup(context: &mut dyn Context, id: i64) -> Option<Rc<Chains>> {
    context.state().get::<Streams>()?.0.get(&id).cloned()
}

pub(crate) async fn attach(
    context: &mut dyn Context,
    stream: &Rc<Resource>,
    function: &str,
    name: &[u8],
    class: &str,
    params: &Value,
    ((read, write), prepend): ((bool, bool), bool),
) -> NativeResult {
    let chains = lookup(context, stream.id).unwrap_or_default();
    if chains.busy.get() {
        return Err(NativeError::error("Stream is already executing a filter"));
    }
    if chains.read.borrow().len()
        + chains.write.borrow().len()
        + usize::from(read)
        + usize::from(write)
        > MAX_STAGES
    {
        context.warning(format!(
            "{function}(): Stream filter chain exceeds Phera's limit of 16 filters"
        ));
        return Ok(Value::Bool(false));
    }
    let Some(class_handle) = context.lookup_class(class).await? else {
        let name = String::from_utf8_lossy(name);
        context.warning(format!("{function}(): User-filter \"{name}\" requires class \"{class}\", but that class is not defined"));
        context.warning(format!(
            "{function}(): Unable to create or locate filter \"{name}\""
        ));
        return Ok(Value::Bool(false));
    };
    let display_name = String::from_utf8_lossy(name);
    let name = Value::from(name.to_vec());
    let mut objects = Vec::new();
    for _ in 0..usize::from(read) + usize::from(write) {
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("filtername"), name.clone());
        properties.insert(ArrayKey::str("params"), params.clone());
        let Value::Object(filter) = context.create_object(class_handle.name(), properties)? else {
            return Err(NativeError::error(
                "Stream filter factory did not create an object",
            ));
        };
        if matches!(
            invoke(context, &filter, "oncreate", Vec::new()).await?,
            Some(Value::Bool(false))
        ) {
            context.warning(format!(
                "{function}(): Unable to create or locate filter \"{display_name}\""
            ));
            return Ok(Value::Bool(false));
        }
        objects.push(filter);
    }
    if stream.is_closed() || objects.is_empty() {
        return Ok(Value::Bool(false));
    }
    let attachment = new_resource(context.state(), "stream filter", Attachment(stream.id));
    let Value::Resource(resource) = &attachment else {
        return Err(NativeError::error(
            "Stream filter resource allocation failed",
        ));
    };
    let mut objects = objects.into_iter();
    for (enabled, stages) in [(read, &chains.read), (write, &chains.write)] {
        if enabled && let Some(filter) = objects.next() {
            let stage = Stage {
                id: resource.id,
                filter,
            };
            let mut stages = stages.borrow_mut();
            if prepend {
                stages.insert(0, stage);
            } else {
                stages.push(stage);
            }
        }
    }
    *chains.stream.borrow_mut() = Rc::downgrade(stream);
    context
        .state()
        .get_or_default::<Streams>()
        .0
        .insert(stream.id, chains);
    context.register_resource_finalizer("user stream filters", finish);
    Ok(attachment)
}

impl Chains {
    pub(crate) fn len(&self) -> usize {
        self.read.borrow().len() + self.write.borrow().len()
    }

    pub(crate) fn reading(&self) -> bool {
        !self.read.borrow().is_empty() || !self.buffered.borrow().is_empty()
    }

    pub(crate) fn writing(&self) -> bool {
        !self.write.borrow().is_empty()
    }

    pub(crate) fn has_buffered(&self) -> bool {
        !self.buffered.borrow().is_empty()
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.busy.get()
    }

    pub(crate) async fn read(
        &self,
        context: &mut dyn Context,
        stream: &Resource,
        function: &str,
        limit: usize,
        line: bool,
    ) -> Result<io::Result<Vec<u8>>, NativeError> {
        let _busy = self.enter()?;
        let stages = self.read.borrow().clone();
        let mut bytes = Vec::new();
        while bytes.len() < limit {
            let (count, newline) = {
                let buffer = self.buffered.borrow();
                let max = (limit - bytes.len()).min(buffer.len());
                let newline = line
                    .then(|| buffer.iter().take(max).position(|&byte| byte == b'\n'))
                    .flatten();
                (newline.map_or(max, |pos| pos + 1), newline.is_some())
            };
            bytes.extend(self.buffered.borrow_mut().drain(..count));
            if newline || bytes.len() == limit || self.exhausted.get() {
                break;
            }
            let block = match crate::transfer::read_unfiltered(
                context,
                stream,
                function,
                Some(crate::io_policy::CHUNK as u64),
                false,
            )
            .await?
            {
                Ok(block) => block,
                Err(error) => return Ok(Err(error)),
            };
            let closing = block.is_empty();
            let Some(output) = self
                .apply(context, &stages, function, block, closing)
                .await?
            else {
                self.exhausted.set(true);
                return Ok(Err(io::ErrorKind::Other.into()));
            };
            self.exhausted.set(closing);
            self.buffered.borrow_mut().extend(output);
        }
        Ok(Ok(bytes))
    }

    pub(crate) async fn write(
        &self,
        context: &mut dyn Context,
        resource: &Resource,
        function: &str,
        bytes: &[u8],
    ) -> NativeResult {
        let _busy = self.enter()?;
        let stages = self.write.borrow().clone();
        let mut count = 0;
        for chunk in bytes.chunks(crate::io_policy::CHUNK) {
            let Some(transformed) = self
                .apply(context, &stages, function, chunk.to_vec(), false)
                .await?
            else {
                return Ok(if count == 0 {
                    Value::Bool(false)
                } else {
                    Value::Int(count as i64)
                });
            };
            match crate::transfer::write_unfiltered(context, resource, &transformed, function)
                .await?
            {
                Value::Int(size) if size as usize == transformed.len() => count += chunk.len(),
                _ => return Ok(Value::Int(count as i64)),
            }
        }
        Ok(Value::Int(count as i64))
    }

    async fn apply(
        &self,
        context: &mut dyn Context,
        stages: &[Stage],
        function: &str,
        mut data: Vec<u8>,
        closing: bool,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        for (index, stage) in stages.iter().enumerate() {
            let stream = self.stream.borrow().upgrade();
            let Some(output) = run(
                context,
                &stage.filter,
                stream,
                function,
                data,
                closing,
                index == 0 && !closing,
            )
            .await?
            else {
                return Ok(None);
            };
            data = output;
        }
        let total = self.produced.get().saturating_add(data.len());
        if total > MAX_OUTPUT {
            return Err(NativeError::error(super::super::limit().to_string()));
        }
        self.produced.set(total);
        Ok(Some(data))
    }

    fn enter(&self) -> Result<Busy<'_>, NativeError> {
        if self.busy.replace(true) {
            return Err(NativeError::error("Stream is already executing a filter"));
        }
        Ok(Busy(&self.busy))
    }
}

struct Busy<'a>(&'a Cell<bool>);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

async fn run(
    context: &mut dyn Context,
    object: &Rc<Object>,
    stream: Option<Rc<Resource>>,
    function: &str,
    data: Vec<u8>,
    closing: bool,
    initial: bool,
) -> Result<Option<Vec<u8>>, NativeError> {
    bounded(data.len()).map_err(|error| NativeError::error(error.to_string()))?;
    let input = buckets::brigade(context, data);
    let output = buckets::brigade(context, Vec::new());
    let consumed = Value::Reference(Rc::new(RefCell::new(if initial {
        Value::Int(0)
    } else {
        Value::Null
    })));
    let key = ArrayKey::str("stream");
    if object.get(&key).is_some() {
        object.set(key.clone(), stream.map_or(Value::Null, Value::Resource));
    }
    let _reset = StreamProperty { object, key };
    let result = invoke(
        context,
        object,
        "filter",
        vec![
            input.clone(),
            output.clone(),
            consumed,
            Value::Bool(closing),
        ],
    )
    .await;
    if buckets::finish(&input).is_some_and(|input| !input.empty()) && result.is_ok() {
        context.warning(format!(
            "{function}(): Unprocessed filter buckets remaining on input brigade"
        ));
    }
    let out = buckets::finish(&output);
    match result? {
        Some(value) => match to_int(&value, context.diagnostics())? {
            PASS => Ok(Some(out.map_or_else(Vec::new, |out| out.bytes()))),
            FEED => Ok(Some(Vec::new())),
            _ => Ok(None),
        },
        _ => {
            context.warning(format!("{function}(): Failed to call filter function"));
            Ok(None)
        }
    }
}

struct StreamProperty<'a> {
    object: &'a Object,
    key: ArrayKey,
}

impl Drop for StreamProperty<'_> {
    fn drop(&mut self) {
        if self.object.get(&self.key).is_some() {
            self.object.set(self.key.clone(), Value::Null);
        }
    }
}

pub(crate) async fn close(
    context: &mut dyn Context,
    resource: &Resource,
) -> Result<(), NativeError> {
    let Some(chains) = lookup(context, resource.id) else {
        return Ok(());
    };
    let _busy = chains.enter()?;
    let stages = chains.write.take();
    let output = chains
        .apply(context, &stages, "fclose", Vec::new(), true)
        .await;
    let result = match output {
        Ok(Some(data)) => crate::transfer::write_unfiltered(context, resource, &data, "fclose")
            .await
            .map(|_| ()),
        Ok(None) => Ok(()),
        Err(error) => Err(error),
    };
    let mut close_error = None;
    for stage in chains.read.take().into_iter().chain(stages) {
        if let Err(error) = invoke(context, &stage.filter, "onclose", Vec::new()).await
            && close_error.is_none()
        {
            close_error = Some(error);
        }
    }
    context
        .state()
        .get_or_default::<Streams>()
        .0
        .remove(&resource.id);
    result.and(close_error.map_or(Ok(()), Err))
}

pub(crate) async fn remove(
    context: &mut dyn Context,
    resource: &Resource,
) -> Result<Option<Value>, NativeError> {
    let stream_id = resource
        .data
        .borrow()
        .as_ref()
        .and_then(|data| data.downcast_ref::<Attachment>())
        .map(|attachment| attachment.0);
    let Some(stream_id) = stream_id else {
        return Ok(None);
    };
    let Some(chains) = lookup(context, stream_id) else {
        return Err(NativeError::type_error(
            "stream_filter_remove(): supplied resource is not a valid stream filter resource",
        ));
    };
    let _busy = chains.enter()?;
    let stream = chains.stream.borrow().upgrade();
    for (read, stages) in [(true, &chains.read), (false, &chains.write)] {
        let remaining = stages.borrow().clone();
        let Some(index) = remaining.iter().position(|stage| stage.id == resource.id) else {
            continue;
        };
        let stage = &remaining[index];
        let function = "stream_filter_remove";
        let Some(out) = run(
            context,
            &stage.filter,
            stream.clone(),
            function,
            Vec::new(),
            true,
            false,
        )
        .await?
        else {
            return Ok(Some(Value::Bool(false)));
        };
        let Some(out) = chains
            .apply(context, &remaining[index + 1..], function, out, false)
            .await?
        else {
            return Ok(Some(Value::Bool(false)));
        };
        if read {
            let mut buffer = chains.buffered.borrow_mut();
            bounded(buffer.len() + out.len())
                .map_err(|error| NativeError::error(error.to_string()))?;
            buffer.extend(out);
        } else if let Some(stream) = &stream {
            crate::transfer::write_unfiltered(context, stream, &out, function).await?;
        }
        stages.borrow_mut().remove(index);
        invoke(context, &stage.filter, "onclose", Vec::new()).await?;
    }
    resource.data.borrow_mut().take();
    Ok(Some(Value::Bool(true)))
}

fn finish(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let streams: Vec<_> = context
            .state()
            .get::<Streams>()
            .map_or_else(Vec::new, |streams| streams.0.values().cloned().collect());
        for chains in streams {
            let resource = chains.stream.borrow().upgrade();
            if let Some(resource) = resource {
                close(context, &resource).await?;
            } else {
                for stage in chains.read.take().into_iter().chain(chains.write.take()) {
                    invoke(context, &stage.filter, "onclose", Vec::new()).await?;
                }
            }
        }
        Ok(Value::Null)
    })
}
