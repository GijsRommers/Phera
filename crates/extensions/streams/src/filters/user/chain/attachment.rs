//! A PHP filter takes over the chain when its first callback stage is attached.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, value::Resource,
};

use crate::{request::new_resource, resource::with_stream, user_file::invoke};

use super::super::super::{Filter, functions::Attachment};
use super::lifecycle::finish;
use super::{Chains, Stage, Streams};

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
}
