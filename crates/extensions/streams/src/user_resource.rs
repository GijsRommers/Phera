//! After `$file = null`, the last owner of a user handle queues `stream_close()`.

use std::{
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Registry, Value,
    value::{Resource, ResourceRelease},
};

use crate::{request, user_directory::UserDirectory, user_file::UserFile};

pub(crate) struct UserHandle<T> {
    pub value: Rc<T>,
    release: Option<ResourceRelease>,
}

enum Handle {
    File(Rc<UserFile>),
    Directory(Rc<UserDirectory>),
}

#[derive(Default)]
struct OpenHandles(BTreeMap<i64, (Weak<Resource>, Handle)>);

pub(crate) fn register(registry: &mut Registry) {
    registry.register_resource_release("stream", close_released);
    registry.fork_state::<OpenHandles>(|state, fork| {
        let handles = state.downcast_ref::<OpenHandles>()?;
        if let Some((_, handle)) = handles.0.values().next() {
            let object = match handle {
                Handle::File(file) => &file.object,
                Handle::Directory(directory) => &directory.object,
            };
            fork.refuse_state(
                object.class.name(),
                "an open user wrapper handle belongs to the boot",
            );
        }
        None
    });
}

pub(crate) fn make_file(context: &mut dyn Context, file: Rc<UserFile>) -> Value {
    make(context, file.clone(), Handle::File(file))
}

pub(crate) fn make_directory(context: &mut dyn Context, dir: Rc<UserDirectory>) -> Value {
    make(context, dir.clone(), Handle::Directory(dir))
}

fn make<T: 'static>(context: &mut dyn Context, value: Rc<T>, handle: Handle) -> Value {
    let id = request::files(context).claim_id();
    let release = context.resource_release("stream", id);
    let resource = request::resource(id, "stream", UserHandle { value, release });
    request::track_standard(context.state(), &resource);
    if let Value::Resource(resource) = &resource {
        context
            .state()
            .get_or_default::<OpenHandles>()
            .0
            .insert(resource.id, (Rc::downgrade(resource), handle));
        context.register_resource_finalizer("user streams", finish);
    }
    resource
}

pub(crate) fn release(context: &mut dyn Context, resource: &Resource) {
    retire(resource);
    context
        .state()
        .get_or_default::<OpenHandles>()
        .0
        .remove(&resource.id);
}

fn retire(resource: &Resource) {
    let mut data = resource.data.borrow_mut();
    if let Some(mut data) = data.take() {
        let release = if let Some(file) = data.downcast_mut::<UserHandle<UserFile>>() {
            file.release.as_mut()
        } else {
            data.downcast_mut::<UserHandle<UserDirectory>>()
                .and_then(|dir| dir.release.as_mut())
        };
        if let Some(release) = release {
            release.cancel();
        }
    }
}

fn close_released(context: &mut dyn Context, id: i64) -> LocalFuture<'_, Result<(), NativeError>> {
    Box::pin(async move {
        let handle = context
            .state()
            .get_or_default::<OpenHandles>()
            .0
            .remove(&id);
        if let Some((_, handle)) = handle {
            close_handle(context, id, handle, None).await?;
        }
        Ok(())
    })
}

async fn close_handle(
    context: &mut dyn Context,
    id: i64,
    handle: Handle,
    resource: Option<Rc<Resource>>,
) -> Result<(), NativeError> {
    match handle {
        Handle::File(value) => {
            let filtered = if crate::filters::user::lookup(context, id).is_some() {
                let resource = resource.unwrap_or_else(|| {
                    Rc::new(Resource::new(
                        id,
                        "stream",
                        Some(Box::new(UserHandle {
                            value: value.clone(),
                            release: None,
                        })),
                    ))
                });
                let result = crate::filters::user::close_released(context, &resource).await;
                retire(&resource);
                result
            } else {
                if let Some(resource) = resource {
                    retire(&resource);
                }
                Ok(())
            };
            let closed = value.close(context).await;
            filtered.and(closed)
        }
        Handle::Directory(value) => {
            if let Some(resource) = resource {
                retire(&resource);
            }
            value.close(context).await.map(drop)
        }
    }
}

fn finish(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut failure = None;
        if let Some(handles) = context.state().take::<OpenHandles>() {
            for (id, (resource, handle)) in handles.0.into_iter().rev() {
                let result = close_handle(context, id, handle, resource.upgrade()).await;
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
