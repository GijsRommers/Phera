//! Each recursive level remembers whether its current node or its children come next.
//!
//! ```php
//! $all = new RecursiveIteratorIterator($tree, RecursiveIteratorIterator::CHILD_FIRST);
//! ```

use phera_extension_api::{
    Context, NativeError, NativeResult, Object, Value, traversal::IteratorStep, value::to_bool,
};
use std::rc::Rc;

use super::{
    Mode, Phase, at, attach, depth, detach, hook, inner, phase, read, step, uninitialized, update,
};

pub(super) async fn has_children(
    context: &mut dyn Context,
    object: &Rc<Object>,
    user_override: bool,
) -> NativeResult {
    let (receiver, method) = if user_override && context.is_user_method(object, "callHasChildren") {
        (object.clone(), "callHasChildren")
    } else {
        (inner(object)?, "hasChildren")
    };
    let result = context.call_method(receiver, method, Vec::new()).await?;
    Ok(Value::Bool(to_bool(&result)))
}

pub(super) async fn children(
    context: &mut dyn Context,
    object: &Rc<Object>,
    user_override: bool,
) -> NativeResult {
    let (receiver, method) = if user_override && context.is_user_method(object, "callGetChildren") {
        (object.clone(), "callGetChildren")
    } else {
        (inner(object)?, "getChildren")
    };
    context.call_method(receiver, method, Vec::new()).await
}

async fn advance(context: &mut dyn Context, object: &Rc<Object>) -> NativeResult {
    let settings = read(object, |state| state.settings)?.ok_or_else(uninitialized)?;
    let catches = settings.catch_children;
    loop {
        let level = depth(object)?;
        let next = read(object, |state| state.levels.last().map(|level| level.phase))?
            .ok_or_else(uninitialized)?;
        match next {
            Phase::Advance => {
                if let Err(error) = step(context, object, level, IteratorStep::Advance).await
                    && !catches
                {
                    return Err(error);
                }
                phase(object, Phase::Start)?;
            }
            Phase::Start => {
                let valid = step(context, object, level, IteratorStep::Valid).await?;
                phase(
                    object,
                    if to_bool(&valid) {
                        Phase::Test
                    } else {
                        Phase::End
                    },
                )?;
            }
            Phase::End => {
                if level == 0 {
                    return Ok(Value::Null);
                }
                if let Err(error) = hook(context, object, "endChildren").await
                    && !catches
                {
                    return Err(error);
                }
                detach(object)?;
            }
            Phase::Test => {
                let has_children = match has_children(context, object, true).await {
                    Ok(children) => to_bool(&children),
                    Err(_) if catches => false,
                    Err(error) => {
                        phase(object, Phase::Advance)?;
                        return Err(error);
                    }
                };
                let max = read(object, |state| state.max_depth)?;
                if has_children && max.is_none_or(|max| (level as i64) < max) {
                    let next = match settings.mode {
                        Mode::Leaves | Mode::PostOrder => Some(Phase::Descend),
                        Mode::PreOrder => Some(Phase::YieldParent),
                        Mode::Other => None,
                    };
                    if let Some(next) = next {
                        phase(object, next)?;
                        continue;
                    }
                }
                phase(object, Phase::Advance)?;
                if has_children && matches!(settings.mode, Mode::Leaves) {
                    continue;
                }
                if let Err(error) = hook(context, object, "nextElement").await
                    && !catches
                {
                    return Err(error);
                }
                return Ok(Value::Null);
            }
            Phase::YieldParent => {
                let next = if matches!(settings.mode, Mode::PreOrder) {
                    Phase::Descend
                } else {
                    Phase::Advance
                };
                hook(context, object, "nextElement").await?;
                phase(object, next)?;
                return Ok(Value::Null);
            }
            Phase::Descend => {
                let child = match children(context, object, true).await {
                    Ok(child) => child,
                    Err(_) if catches => {
                        phase(object, Phase::Advance)?;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let Value::Object(child) = &*child.deref() else {
                    return Err(invalid_child());
                };
                if !child.class.is_instance_of("RecursiveIterator") {
                    return Err(invalid_child());
                }
                let parent_next = if matches!(settings.mode, Mode::PostOrder) {
                    Phase::YieldParent
                } else {
                    Phase::Advance
                };
                phase(object, parent_next)?;
                attach(context, object, child.clone()).await?;
                step(context, object, level + 1, IteratorStep::Rewind).await?;
                if let Err(error) = hook(context, object, "beginChildren").await
                    && !catches
                {
                    return Err(error);
                }
            }
        }
    }
}

fn invalid_child() -> NativeError {
    NativeError::throw(
        "UnexpectedValueException",
        "Objects returned by RecursiveIterator::getChildren() must implement RecursiveIterator",
    )
}

async fn rewind_root(context: &mut dyn Context, object: &Rc<Object>) -> Result<(), NativeError> {
    while depth(object)? > 0 {
        hook(context, object, "endChildren").await?;
        detach(object)?;
    }
    phase(object, Phase::Start)?;
    step(context, object, 0, IteratorStep::Rewind).await?;
    if !read(object, |state| state.in_iteration)? {
        hook(context, object, "beginIteration").await?;
    }
    update(object, |state| state.in_iteration = true)
}

pub(super) async fn run(
    context: &mut dyn Context,
    object: &Rc<Object>,
    rewind: bool,
) -> NativeResult {
    update(object, |state| {
        if state.running {
            return Err(NativeError::error(
                "Cannot re-enter an active recursive iterator",
            ));
        }
        state.running = true;
        Ok(())
    })??;
    let result = async {
        if rewind {
            rewind_root(context, object).await?;
        }
        advance(context, object).await
    }
    .await;
    update(object, |state| state.running = false)?;
    result
}

pub(super) async fn valid(context: &mut dyn Context, object: &Rc<Object>) -> NativeResult {
    for level in (0..=depth(object)?).rev() {
        if at(object, level)?.is_none() {
            continue;
        }
        if to_bool(&step(context, object, level, IteratorStep::Valid).await?) {
            return Ok(Value::Bool(true));
        }
    }
    if read(object, |state| state.in_iteration)? {
        hook(context, object, "endIteration").await?;
        update(object, |state| state.in_iteration = false)?;
    }
    Ok(Value::Bool(false))
}
