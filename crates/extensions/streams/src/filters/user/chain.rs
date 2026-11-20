//! Read and write filters retain their own PHP object while the stream stays open.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    io,
    rc::{Rc, Weak},
};

use phera_extension_api::{Context, NativeError, NativeResult, Value, value::Resource};

use crate::resource::with_stream;

use super::super::Flush;
use super::stage::{Stage, StageOutput};

mod attachment;
mod lifecycle;

pub(in crate::filters) use attachment::{Placement, attach, attach_native};
pub(in crate::filters) use lifecycle::remove;
pub(crate) use lifecycle::{close, close_released};

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
            let eof = if let Some(file) = crate::user_file::resource_file(stream) {
                file.eof.get()
            } else {
                with_stream(stream, |stream| stream.eof)?
            };
            let closing = eof || block.is_empty();
            let Some(output) = self
                .apply(
                    context,
                    &stages,
                    function,
                    block,
                    if closing { Flush::Closing } else { Flush::No },
                    false,
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
                .apply(context, &stages, function, chunk.to_vec(), Flush::No, true)
                .await?
            else {
                return Ok(if count == 0 {
                    Value::Bool(false)
                } else {
                    Value::Int(count as i64)
                });
            };
            if transformed.is_empty() {
                count += chunk.len();
                continue;
            }
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
        count_input: bool,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        for (index, stage) in stages.iter().enumerate() {
            let stream = self.stream.borrow().upgrade();
            match stage
                .apply(
                    context,
                    stream,
                    function,
                    data,
                    flush,
                    index == 0 && count_input,
                )
                .await?
            {
                StageOutput::Pass(output) => data = output,
                StageOutput::Feed if flush == Flush::No => return Ok(Some(Vec::new())),
                StageOutput::Feed => data = Vec::new(),
                StageOutput::Failed => return Ok(None),
            }
        }
        Ok(Some(data))
    }

    pub(crate) async fn flush(
        &self,
        context: &mut dyn Context,
        resource: &Resource,
        function: &str,
    ) -> Result<bool, NativeError> {
        let _busy = self.enter()?;
        let stages = self.write.borrow().clone();
        let Some(output) = self
            .apply(
                context,
                &stages,
                function,
                Vec::new(),
                Flush::Incremental,
                true,
            )
            .await?
        else {
            return Ok(false);
        };
        if output.is_empty() {
            return Ok(true);
        }
        let result =
            crate::transfer::write_unfiltered(context, resource, &output, function).await?;
        Ok(matches!(result, Value::Int(written) if written as usize == output.len()))
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
