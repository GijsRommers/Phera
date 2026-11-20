//! An unclosed user stream still receives `stream_close()` at request teardown.

use std::{
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use phera_extension_api::{Context, LocalFuture, NativeResult, Value, value::Resource};

use crate::{request::new_resource, user_directory::UserDirectory, user_file::UserFile};

enum Handle {
    File(Rc<UserFile>),
    Directory(Rc<UserDirectory>),
}

#[derive(Default)]
struct OpenHandles(BTreeMap<i64, (Weak<Resource>, Handle)>);

pub(crate) fn make_file(context: &mut dyn Context, file: Rc<UserFile>) -> Value {
    let resource = new_resource(context.state(), "stream", file.clone());
    track(context, &resource, Handle::File(file));
    resource
}

pub(crate) fn make_directory(context: &mut dyn Context, dir: Rc<UserDirectory>) -> Value {
    let resource = new_resource(context.state(), "stream", dir.clone());
    track(context, &resource, Handle::Directory(dir));
    resource
}

fn track(context: &mut dyn Context, resource: &Value, handle: Handle) {
    if let Value::Resource(resource) = resource {
        context
            .state()
            .get_or_default::<OpenHandles>()
            .0
            .insert(resource.id, (Rc::downgrade(resource), handle));
        context.register_resource_finalizer("user streams", finish);
    }
}

pub(crate) fn release(context: &mut dyn Context, id: i64) {
    context
        .state()
        .get_or_default::<OpenHandles>()
        .0
        .remove(&id);
}

fn finish(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut failure = None;
        if let Some(handles) = context.state().take::<OpenHandles>() {
            for (_, (resource, handle)) in handles.0.into_iter().rev() {
                if let Some(resource) = resource.upgrade() {
                    resource.data.borrow_mut().take();
                }
                let result = match handle {
                    Handle::File(file) => file.close(context).await,
                    Handle::Directory(dir) => dir.close(context).await.map(|_| ()),
                };
                if let Err(error) = result
                    && failure.is_none()
                {
                    failure = Some(error);
                }
            }
        }
        failure.map_or(Ok(Value::Null), Err)
    })
}
