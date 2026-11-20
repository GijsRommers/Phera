//! `property_get` describes a Closure's signature without calling it.

use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::CallTarget;
use crate::closures::ClosureState;

use super::properties::Property;

pub(super) fn children(vm: &Vm, object: &Object, full: &str) -> Vec<Property> {
    let Ok(native) = object.native.try_borrow() else {
        return Vec::new();
    };
    let Some(closure) = native
        .as_deref()
        .and_then(|state| state.downcast_ref::<ClosureState>())
    else {
        return Vec::new();
    };
    let mut name = closure.callee.name.to_string();
    let mut properties = Vec::new();
    let php = match &closure.callee.target {
        CallTarget::User(function) => Some(*function),
        CallTarget::Method { method, .. } => match method.body {
            crate::classes::MethodBody::User(function) => Some(function),
            _ => None,
        },
        _ => None,
    };
    if let Some(index) = php {
        let function = vm.code.function(index);
        let file = vm.code.unit(index).file.as_str();
        let line = function.definition_line;
        if name.starts_with("{closure:") {
            name = format!("{{closure:{file}:{line}}}");
        }
        properties.push(Property::member(full, "file", Value::from(file)));
        properties.push(Property::member(full, "line", Value::Int(i64::from(line))));
    }
    properties.insert(0, Property::member(full, "name", Value::from(name)));
    if !closure.callee.parameters.is_empty() {
        let mut params = PhpArray::new();
        for param in closure.callee.parameters.iter() {
            let name = format!("${}", param.name);
            params.insert(ArrayKey::str(name), Value::from("<required>"));
        }
        properties.push(Property::member(full, "parameter", Value::from(params)));
    }
    properties
}
