//! A native codec and a PHP `php_user_filter` can occupy the same pipeline.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    ArrayKey, Context, NativeError, Object, Value,
    value::{Resource, to_int},
};

use crate::{resource::with_stream, user_file::invoke};

use super::super::{Filter, Flush, bounded};
use super::{FEED, PASS, buckets};

#[derive(Clone)]
pub(super) struct Stage {
    pub id: i64,
    filter: Kind,
}

#[derive(Clone)]
enum Kind {
    Php(Rc<Object>),
    Native(Rc<RefCell<Filter>>),
}

impl Stage {
    pub fn php(id: i64, filter: Rc<Object>) -> Self {
        Self {
            id,
            filter: Kind::Php(filter),
        }
    }

    pub fn native(filter: Filter) -> Self {
        Self {
            id: filter.id,
            filter: Kind::Native(Rc::new(RefCell::new(filter))),
        }
    }

    pub fn name(&self) -> &str {
        match &self.filter {
            Kind::Php(object) => object.class.name(),
            Kind::Native(_) => "stream filter",
        }
    }

    pub async fn apply(
        &self,
        context: &mut dyn Context,
        stream: Option<Rc<Resource>>,
        function: &str,
        data: Vec<u8>,
        flush: Flush,
        initial: bool,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        match &self.filter {
            Kind::Php(filter) => {
                run(
                    context,
                    filter,
                    stream,
                    function,
                    data,
                    flush == Flush::Closing,
                    initial,
                )
                .await
            }
            Kind::Native(filter) => {
                let (result, seek) = {
                    let mut filter = filter.borrow_mut();
                    let position = stream.as_ref().map_or(0, |stream| {
                        if let Some(file) = crate::user_file::resource_file(stream) {
                            file.position.get() as u64
                        } else {
                            with_stream(stream, |stream| stream.position).unwrap_or(0)
                        }
                    });
                    let seek = filter.codec.seek_after(data.len(), flush, position);
                    (filter.codec.apply(data, flush), seek)
                };
                if let Some((stream, position)) = stream.zip(seek) {
                    if let Some(file) = crate::user_file::resource_file(&stream) {
                        file.seek(context, function, position as i64, 0).await?;
                    } else {
                        with_stream(&stream, |stream| {
                            let _ = stream.seek(std::io::SeekFrom::Start(position));
                        })?;
                    }
                }
                match result.and_then(|output| {
                    bounded(output.len())?;
                    Ok(output)
                }) {
                    Ok(output) => Ok(Some(output)),
                    Err(error) => {
                        context.warning(format!("{function}(): {error}"));
                        Ok(None)
                    }
                }
            }
        }
    }

    pub async fn close(&self, context: &mut dyn Context) -> Result<(), NativeError> {
        if let Kind::Php(filter) = &self.filter {
            invoke(context, filter, "onclose", Vec::new()).await?;
        }
        Ok(())
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
    if buckets::finish(&input).is_some_and(|input| !input.empty()) {
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
