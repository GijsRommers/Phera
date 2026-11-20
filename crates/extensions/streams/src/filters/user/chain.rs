//! Read and write filters retain their own PHP object while the stream stays open.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    io,
    rc::{Rc, Weak},
};

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, value::Resource,
};

use crate::{request::new_resource, resource::with_stream, user_file::invoke};

use super::super::{Filter, Flush, bounded, functions::Attachment};
use super::stage::Stage;

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
                    stage.name(),
                    "an attached stream filter belongs to the boot",
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
    stream: RefCell<Weak<Resource>>,
}

pub(crate) fn lookup(context: &mut dyn Context, id: i64) -> Option<Rc<Chains>> {
    context.state().get::<Streams>()?.0.get(&id).cloned()
}

#[derive(Clone, Copy)]
pub(in crate::filters) enum Placement {
    Append,
    Prepend,
    Uri,
}

fn mixed(context: &mut dyn Context, stream: &Rc<Resource>) -> Rc<Chains> {
    let chains = context
        .state()
        .get_or_default::<Streams>()
        .0
        .entry(stream.id)
        .or_default()
        .clone();
    *chains.stream.borrow_mut() = Rc::downgrade(stream);
    context.register_resource_finalizer("user stream filters", finish);
    chains
}

pub(in crate::filters) async fn attach(
    context: &mut dyn Context,
    stream: &Rc<Resource>,
    function: &str,
    name: &[u8],
    class: &str,
    params: &Value,
    ((read, write), placement): ((bool, bool), Placement),
) -> NativeResult {
    if !read && !write {
        return Ok(Value::Bool(false));
    }
    let chains = mixed(context, stream);
    let _busy = chains.enter()?;
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
    let attachment = if matches!(placement, Placement::Uri) {
        Value::Bool(true)
    } else {
        new_resource(
            context.state(),
            "stream filter",
            Attachment(Rc::downgrade(stream)),
        )
    };
    let id = match &attachment {
        Value::Resource(resource) => resource.id,
        _ => 0,
    };
    chains.promote(stream)?;
    let mut objects = objects.into_iter();
    for (enabled, stages) in [(read, &chains.read), (write, &chains.write)] {
        if enabled && let Some(filter) = objects.next() {
            Chains::insert(stages, Stage::php(id, filter), placement);
        }
    }
    Ok(attachment)
}

pub(in crate::filters) fn attach_native(
    context: &mut dyn Context,
    stream: &Rc<Resource>,
    read: Option<Filter>,
    write: Option<Filter>,
    placement: Placement,
) -> Result<(), NativeError> {
    let chains = mixed(context, stream);
    let _busy = chains.enter()?;
    chains.promote(stream)?;
    if let Some(filter) = read {
        Chains::insert(&chains.read, Stage::native(filter), placement);
    }
    if let Some(filter) = write {
        Chains::insert(&chains.write, Stage::native(filter), placement);
    }
    Ok(())
}

impl Chains {
    pub(in crate::filters) fn contains(&self, id: i64) -> bool {
        self.read.borrow().iter().any(|stage| stage.id == id)
            || self.write.borrow().iter().any(|stage| stage.id == id)
    }
    fn promote(&self, resource: &Resource) -> Result<(), NativeError> {
        if crate::user_file::resource_file(resource).is_none()
            && let Some(filters) = with_stream(resource, |stream| stream.filters.take())?
        {
            self.read
                .borrow_mut()
                .extend(filters.read.into_filters().into_iter().map(Stage::native));
            self.write
                .borrow_mut()
                .extend(filters.write.into_filters().into_iter().map(Stage::native));
            *self.buffered.borrow_mut() = filters.buffered;
            self.exhausted.set(filters.read_closed);
        }
        Ok(())
    }

    fn insert(stages: &RefCell<Vec<Stage>>, stage: Stage, placement: Placement) {
        let mut stages = stages.borrow_mut();
        match placement {
            Placement::Prepend => stages.insert(0, stage),
            Placement::Append | Placement::Uri => stages.push(stage),
        }
    }

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
                .apply(
                    context,
                    &stages,
                    function,
                    block,
                    if closing { Flush::Closing } else { Flush::No },
                )
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
                .apply(context, &stages, function, chunk.to_vec(), Flush::No)
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
        flush: Flush,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        for (index, stage) in stages.iter().enumerate() {
            let stream = self.stream.borrow().upgrade();
            let Some(output) = stage
                .apply(
                    context,
                    stream,
                    function,
                    data,
                    flush,
                    index == 0 && flush == Flush::No,
                )
                .await?
            else {
                return Ok(None);
            };
            data = output;
        }
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

pub(crate) async fn close_released(
    context: &mut dyn Context,
    resource: &Rc<Resource>,
) -> Result<(), NativeError> {
    if let Some(chains) = lookup(context, resource.id) {
        *chains.stream.borrow_mut() = Rc::downgrade(resource);
    }
    close(context, resource).await
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
        .apply(context, &stages, "fclose", Vec::new(), Flush::Closing)
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
        if let Err(error) = stage.close(context).await
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

pub(in crate::filters) async fn remove(
    context: &mut dyn Context,
    resource: &Resource,
    chains: &Chains,
) -> NativeResult {
    let _busy = chains.enter()?;
    let stream = chains.stream.borrow().upgrade();
    for (read, stages) in [(true, &chains.read), (false, &chains.write)] {
        let remaining = stages.borrow().clone();
        let Some(index) = remaining.iter().position(|stage| stage.id == resource.id) else {
            continue;
        };
        let stage = &remaining[index];
        let function = "stream_filter_remove";
        let Some(out) = stage
            .apply(
                context,
                stream.clone(),
                function,
                Vec::new(),
                Flush::Closing,
                false,
            )
            .await?
        else {
            return Ok(Value::Bool(false));
        };
        let Some(out) = chains
            .apply(context, &remaining[index + 1..], function, out, Flush::No)
            .await?
        else {
            return Ok(Value::Bool(false));
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
        stage.close(context).await?;
    }
    resource.data.borrow_mut().take();
    Ok(Value::Bool(true))
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
                    stage.close(context).await?;
                }
            }
        }
        Ok(Value::Null)
    })
}
