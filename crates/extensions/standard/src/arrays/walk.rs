//! `array_walk` and `array_walk_recursive`, which give the callback each
//! value by reference.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    Value, args,
};

type Cell = Rc<RefCell<Value>>;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ARRAY_WALK, ARRAY_WALK_RECURSIVE];

/// Whether nested arrays are walked into.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    Shallow,
    Recursive,
}

/// `array_walk(array|object &$array, callable $callback, mixed $arg):
/// true`, calling `$callback($value, $key[, $arg])` for each entry; a
/// callback that takes `&$value` changes the array.
///
/// ```php
/// $a = [1, 2];
/// array_walk($a, function (&$value, $key) { $value *= 10; }); // $a is [10, 20]
/// ```
const ARRAY_WALK: FunctionEntry = FunctionEntry::asynchronous(
    "array_walk",
    &[
        // array|object, which a parameter list cannot declare; walk() checks it.
        Param::new("array", ParamType::Mixed).by_ref(),
        Param::new("callback", ParamType::Callable),
        Param::new("arg", ParamType::Mixed).optional(),
    ],
    array_walk,
);

fn array_walk(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(walk(context, arguments, Depth::Shallow))
}

/// `array_walk_recursive()`, `array_walk()` going into nested arrays and
/// calling the callback for the values that are not arrays.
///
/// ```php
/// $a = [1, [2, 3]];
/// array_walk_recursive($a, function (&$value) { $value++; }); // $a is [2, [3, 4]]
/// ```
const ARRAY_WALK_RECURSIVE: FunctionEntry = FunctionEntry::asynchronous(
    "array_walk_recursive",
    &[
        Param::new("array", ParamType::Mixed).by_ref(),
        Param::new("callback", ParamType::Callable),
        Param::new("arg", ParamType::Mixed).optional(),
    ],
    array_walk_recursive,
);

fn array_walk_recursive(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(walk(context, arguments, Depth::Recursive))
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
        Ok(Value::from(key.clone()))
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

async fn walk(context: &mut dyn Context, arguments: Vec<Value>, depth: Depth) -> NativeResult {
    let name = if depth == Depth::Recursive {
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
                crate::arguments::given_type(&value)
            )));
        }
    }
    let callback = args::value(&arguments, 1);
    super::callback::check_callback(name, callback)?;
    let mut active = HashSet::from([Rc::as_ptr(&target)]);
    let mut frames = vec![Frame::new(target)?];
    while let Some(frame) = frames.last_mut() {
        let Some(key) = frame.keys.next() else {
            frame.ensure_container()?;
            active.remove(&Rc::as_ptr(&frame.target));
            frames.pop();
            continue;
        };
        let Some(element) = frame.element(&key)? else {
            continue;
        };
        let is_array = matches!(
            &*element.try_borrow().map_err(|_| borrowed())?,
            Value::Array(_)
        );
        if depth == Depth::Recursive && is_array {
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
