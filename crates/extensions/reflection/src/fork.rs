//! Reflection objects in a fork of a snapshot.
//!
//! ```php
//! $property = new ReflectionProperty(Post::class, 'title');   // metadata only
//! $method = new ReflectionFunction(fn () => $this->n);        // refers to a closure
//! Phera\snapshot();
//! // each fork gets its own reflection objects: the same metadata, and the
//! // fork's own copy of the closure and of its $this
//! ```
//!
//! What a reflection object knows about a class or function is metadata
//! that never changes and is shared. What it refers to weakly (a closure,
//! the object a `ReflectionObject` describes, a closure's `$this`) is
//! replaced by the fork's copy.

use std::any::Any;
use std::rc::{Rc, Weak};

use phera_extension_api::Object;
use phera_extension_api::reflection::{
    ConstantInfo, ExtensionInfo, FunctionInfo, FunctionTarget, MemberInfo,
};
use phera_extension_api::value::Fork;

use crate::attribute::ReflectedAttribute;
use crate::class::ReflectedClass;
use crate::parameter::ReflectedParameter;
use crate::types::ReflectedType;

/// The fork hook of every reflection class.
pub(crate) fn fork_state(
    _object: &Object,
    state: &dyn Any,
    fork: &mut Fork<'_>,
) -> Result<Box<dyn Any>, String> {
    if let Some(member) = state.downcast_ref::<Rc<MemberInfo>>() {
        return Ok(Box::new(member.clone()));
    }
    if let Some(class) = state.downcast_ref::<ReflectedClass>() {
        return Ok(Box::new(ReflectedClass {
            info: class.info.clone(),
            object: class.object.as_ref().map(|object| weak(object, fork)),
        }));
    }
    if let Some(function) = state.downcast_ref::<Rc<FunctionInfo>>() {
        return Ok(Box::new(function_info(function, fork)));
    }
    if let Some(parameter) = state.downcast_ref::<ReflectedParameter>() {
        return Ok(Box::new(ReflectedParameter {
            function: function_info(&parameter.function, fork),
            position: parameter.position,
        }));
    }
    if let Some(attribute) = state.downcast_ref::<ReflectedAttribute>() {
        return Ok(Box::new(attribute.clone()));
    }
    if let Some(constant) = state.downcast_ref::<Rc<ConstantInfo>>() {
        return Ok(Box::new(constant.clone()));
    }
    if let Some(extension) = state.downcast_ref::<ExtensionInfo>() {
        return Ok(Box::new(extension.clone()));
    }
    if let Some(reflected) = state.downcast_ref::<ReflectedType>() {
        return Ok(Box::new(reflected.clone()));
    }
    if let Some(id) = state.downcast_ref::<[u8; 20]>() {
        return Ok(Box::new(*id));
    }
    if let Some(generator) = state.downcast_ref::<Weak<Object>>() {
        return Ok(Box::new(weak(generator, fork)));
    }
    Err("its reflection state cannot be copied".into())
}

/// A function's metadata, with the fork's copies of the closure it
/// describes and of that closure's `$this`.
fn function_info(function: &Rc<FunctionInfo>, fork: &mut Fork<'_>) -> Rc<FunctionInfo> {
    let refers = matches!(
        function.target,
        FunctionTarget::Closure(_) | FunctionTarget::ClosureInvoke(_)
    ) || function.closure_this.is_some();
    if !refers {
        return function.clone();
    }
    let mut copy = FunctionInfo::clone(function);
    copy.target = match &function.target {
        FunctionTarget::Closure(closure) => FunctionTarget::Closure(weak(closure, fork)),
        FunctionTarget::ClosureInvoke(closure) => {
            FunctionTarget::ClosureInvoke(weak(closure, fork))
        }
        other => other.clone(),
    };
    copy.closure_this = function.closure_this.as_ref().map(|this| weak(this, fork));
    Rc::new(copy)
}

/// A weak handle on the fork's copy of the object, if it is still alive.
fn weak(object: &Weak<Object>, fork: &mut Fork<'_>) -> Weak<Object> {
    object
        .upgrade()
        .map_or_else(Weak::new, |object| Rc::downgrade(&fork.object(&object)))
}
