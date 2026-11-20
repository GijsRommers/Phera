//! A CLI script ending with `popen('cat >/dev/null', 'w')` closes stdin and reaps cat.

use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use phera_ext_streams::{CloseAttachment, set_attachment, wait_with_execution_limit};
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, ProcessCancellation, Value, value::Resource,
};

use super::{ManagedChild, reap};

#[derive(Default)]
struct Children(BTreeMap<i64, OpenChild>);

struct OpenChild {
    pipe: Weak<Resource>,
    child: ManagedChild,
}

struct AttachedChild(i64);

pub(super) fn track(
    context: &mut dyn Context,
    pipe: &Rc<Resource>,
    child: ManagedChild,
) -> Result<(), NativeError> {
    set_attachment(pipe, Box::new(AttachedChild(pipe.id)))?;
    context.state().get_or_default::<Children>().0.insert(
        pipe.id,
        OpenChild {
            pipe: Rc::downgrade(pipe),
            child,
        },
    );
    context.register_resource_finalizer("popen", finish);
    Ok(())
}

impl CloseAttachment for AttachedChild {
    fn close(self: Box<Self>, context: &mut dyn Context) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let child = context
                .state()
                .get_or_default::<Children>()
                .0
                .remove(&self.0);
            match child {
                Some(child) => reap(context, child.child).await,
                None => Ok(Value::Int(-1)),
            }
        })
    }
}

fn finish(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(children) = context.state().take::<Children>() else {
            return Ok(Value::Null);
        };
        // Close every pipe before doing any waits. One child can depend on
        // another receiving EOF even while both streams are still in PHP variables.
        for open in children.0.values() {
            if let Some(pipe) = open.pipe.upgrade() {
                pipe.data.borrow_mut().take();
            }
        }
        let mut failure = None;
        let mut cancelled = context
            .state()
            .get::<ProcessCancellation>()
            .is_some_and(ProcessCancellation::is_cancelled);
        for mut open in children.0.into_values() {
            if !cancelled {
                match wait_with_execution_limit(context, open.child.wait()).await {
                    Ok(_) => continue,
                    Err(error) => {
                        cancelled = true;
                        failure = Some(error);
                    }
                }
            }
            let _ = open.child.child.kill().await;
        }
        failure.map_or(Ok(Value::Null), Err)
    })
}

/// A child process started before `Phera\snapshot()` belongs to one
/// request: a fork of a snapshot refuses it, and starts without children.
pub(crate) fn register_fork(registry: &mut phera_extension_api::Registry) {
    registry.fork_state::<Children>(fork_children);
}

fn fork_children(
    state: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Option<Box<dyn std::any::Any>> {
    if state
        .downcast_ref::<Children>()
        .is_some_and(|children| !children.0.is_empty())
    {
        fork.refuse_state(
            "process",
            "a child process started before the call belongs to one request",
        );
    }
    None
}
