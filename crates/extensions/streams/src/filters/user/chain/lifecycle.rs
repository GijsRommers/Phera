//! Closing or removing a filter drains staged writes and releases PHP objects.

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Value, value::Resource,
};

use crate::{handles::finish_native, resource::restore_unfiltered};

use super::super::super::{Flush, bounded};
use super::{Chains, StageOutput, Streams, lookup};

pub(crate) async fn close_released(
    context: &mut dyn Context,
    resource: &Rc<Resource>,
) -> Result<(), NativeError> {
    if let Some(chains) = lookup(context, resource.id) {
        *chains.stream.borrow_mut() = Rc::downgrade(resource);
        chains.released.set(true);
    }
    close(context, resource).await
}

pub(crate) async fn close_last_native_owner(
    context: &mut dyn Context,
    id: i64,
) -> Result<(), NativeError> {
    let Some(chains) = lookup(context, id) else {
        return Ok(());
    };
    let Some(backing) = chains.backing.take() else {
        return Ok(());
    };
    let stream = backing.borrow_mut().take();
    let Some(stream) = stream else {
        return Ok(());
    };
    let resource = Rc::new(Resource::new(id, "stream", Some(stream)));
    let filtered = close_released(context, &resource).await;
    let closed = finish_native(context, &resource, "fclose").await;
    filtered.and(closed.map(drop))
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
        .apply(context, &stages, "fclose", Vec::new(), Flush::Closing, true)
        .await;
    let result = match output {
        Ok(Some(data)) if !data.is_empty() => {
            crate::transfer::write_unfiltered(context, resource, &data, "fclose")
                .await
                .map(|_| ())
        }
        Ok(_) => Ok(()),
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
        let out = match stage
            .apply(
                context,
                stream.clone(),
                function,
                Vec::new(),
                Flush::Closing,
                false,
            )
            .await?
        {
            StageOutput::Pass(out) => out,
            StageOutput::Feed => Vec::new(),
            StageOutput::Failed => return Ok(Value::Bool(false)),
        };
        let Some(out) = chains
            .apply(
                context,
                &remaining[index + 1..],
                function,
                out,
                Flush::No,
                false,
            )
            .await?
        else {
            return Ok(Value::Bool(false));
        };
        if read {
            let mut buffer = chains.buffered.borrow_mut();
            bounded(buffer.len() + out.len())
                .map_err(|error| NativeError::error(error.to_string()))?;
            buffer.extend(out);
        } else if !out.is_empty()
            && let Some(stream) = &stream
        {
            crate::transfer::write_unfiltered(context, stream, &out, function).await?;
        }
        stages.borrow_mut().remove(index);
        stage.close(context).await?;
    }
    resource.data.borrow_mut().take();
    if let Some(stream) = stream
        && !chains.has_php()
        && chains.backing.take().is_some()
    {
        restore_unfiltered(&stream);
    }
    Ok(Value::Bool(true))
}

pub(super) fn finish(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
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
