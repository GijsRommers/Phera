//! Walking writes through references: `array_walk($a, function (&$v) { $v *= 2; })`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    Registry, Value, args,
};

type Cell = Rc<RefCell<Value>>;

const PARAMS: &[Param] = &[
    // The API has no ArrayOrObject type; validate this union in the handler.
    Param::new("array", ParamType::Mixed).by_ref(),
    Param::new("callback", ParamType::Callable),
    Param::new("arg", ParamType::Mixed).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous("array_walk", PARAMS, walk));
    registry.function(FunctionEntry::asynchronous(
        "array_walk_recursive",
        PARAMS,
        recursive,
    ));
}

fn walk(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, false))
}
fn recursive(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, true))
}

// Snapshot keys per level so callbacks that keep appending cannot prolong the
// walk forever. PHP's live iteration of appended/replaced keys is not implemented.
struct Frame {
    target: Cell,
    keys: std::vec::IntoIter<ArrayKey>,
}

impl Frame {
    fn new(target: Cell) -> Result<Self, NativeError> {
        let keys = {
            let value = target.try_borrow().map_err(|_| borrowed())?;
            match &*value {
                Value::Array(array) => array.keys().collect::<Vec<_>>(),
                Value::Object(object) => object.to_array().keys().collect(),
                _ => return Err(changed_type()),
            }
        };
        Ok(Self {
            target,
            keys: keys.into_iter(),
        })
    }

    fn ensure_container(&self) -> Result<(), NativeError> {
        let value = self.target.try_borrow().map_err(|_| borrowed())?;
        if matches!(&*value, Value::Array(_) | Value::Object(_)) {
            Ok(())
        } else {
            Err(changed_type())
        }
    }

    fn key_value(&self, key: &ArrayKey) -> Result<Value, NativeError> {
        let value = self.target.try_borrow().map_err(|_| borrowed())?;
        if matches!(&*value, Value::Object(_))
            && let ArrayKey::Int(number) = key
        {
            return Ok(Value::from(number.to_string()));
        }
        Ok(super::key_value(key))
    }

    /// Install the cell before calling PHP. Never hold a table or cell borrow
    /// across an await; deleting/replacing the containing array is then safe.
    fn element(&self, key: &ArrayKey) -> Result<Option<Cell>, NativeError> {
        let mut value = self.target.try_borrow_mut().map_err(|_| borrowed())?;
        match &mut *value {
            Value::Array(array) => {
                let Some(element) = Rc::make_mut(array).get_mut(key) else {
                    return Ok(None);
                };
                Ok(Some(reference(element)))
            }
            Value::Object(object) => {
                let Some(mut element) = object.get(key) else {
                    return Ok(None);
                };
                let reference = reference(&mut element);
                object.set(key.clone(), element);
                Ok(Some(reference))
            }
            _ => Err(changed_type()),
        }
    }
}

fn reference(value: &mut Value) -> Cell {
    if let Value::Reference(cell) = value {
        return cell.clone();
    }
    let cell = Rc::new(RefCell::new(value.clone()));
    *value = Value::Reference(cell.clone());
    cell
}

fn borrowed() -> NativeError {
    NativeError::error("Array is already being modified")
}
fn changed_type() -> NativeError {
    NativeError::type_error("Iterated value is no longer an array or object")
}

async fn run(context: &mut dyn Context, arguments: Vec<Value>, recursive: bool) -> NativeResult {
    let name = if recursive {
        "array_walk_recursive"
    } else {
        "array_walk"
    };
    let target = args::reference(&arguments, 0)
        .ok_or_else(|| NativeError::error("Expected array reference"))?
        .clone();
    {
        let value = target.try_borrow().map_err(|_| borrowed())?;
        if !matches!(&*value, Value::Array(_) | Value::Object(_)) {
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #1 ($array) must be of type array, {} given",
                crate::argument_type_name(&value)
            )));
        }
    }
    let callback = args::value(&arguments, 1);
    super::callback::validate(name, 2, true, callback, false)?;
    // PHP protects an array when walking into it as an element. A root
    // that contains itself is therefore visited once more before the error.
    let mut active = HashSet::new();
    let mut frames = vec![Frame::new(target)?];
    while let Some(frame) = frames.last_mut() {
        let Some(key) = frame.keys.next() else {
            frame.ensure_container()?;
            let finished = Rc::as_ptr(&frame.target);
            frames.pop();
            if !frames.is_empty() {
                active.remove(&finished);
            }
            continue;
        };
        let Some(element) = frame.element(&key)? else {
            continue;
        };
        let is_array = matches!(
            &*element.try_borrow().map_err(|_| borrowed())?,
            Value::Array(_)
        );
        if recursive && is_array {
            if !active.insert(Rc::as_ptr(&element)) {
                return Err(NativeError::error("Recursion detected"));
            }
            frames.push(Frame::new(element)?);
        } else {
            let mut parameters = vec![Value::Reference(element), frame.key_value(&key)?];
            if let Some(extra) = arguments.get(2) {
                parameters.push(extra.clone());
            }
            context.call(callback, parameters).await?;
            frame.ensure_container()?;
        }
    }
    Ok(Value::Bool(true))
}
