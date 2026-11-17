//! Native date objects store an instant and timezone separately, as in `$dt->setTimezone($tz)`.
mod catalog;
mod datetime;
mod registration;
mod timezone;
mod zone_kind;

pub(crate) use registration::register;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value};
use std::rc::Rc;

type This<'a> = Option<&'a Rc<Object>>;

fn receiver(this: This<'_>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Missing date method receiver"))
}

fn state<T: Clone + 'static>(object: &Object, base: &str) -> Result<T, NativeError> {
    object.native.borrow().as_ref().and_then(|s| s.downcast_ref::<T>()).cloned()
        .ok_or_else(|| {
            let name = object.class.name();
            let kind = if name == base { name.to_string() } else { format!("{name} (inheriting {base})") };
            NativeError::throw("DateObjectError", format!(
                "Object of type {kind} has not been correctly initialized by calling parent::__construct() in its constructor"
            ))
        })
}

fn store<T: 'static>(object: &Object, value: T) {
    *object.native.borrow_mut() = Some(Box::new(value));
}

fn make<T: 'static>(ctx: &mut dyn Context, class: &str, value: T) -> NativeResult {
    let object = ctx.create_object(class, PhpArray::new())?;
    if let Value::Object(o) = &object {store(o, value);}
    Ok(object)
}

/// The method and its procedural alias share one implementation and receive the PHP call site.
struct Call<'a> {
    ctx: &'a mut dyn Context,
    this: This<'a>,
    args: &'a [Value],
    class: &'static str,
    site: &'static str,
}

impl Call<'_> {
    fn int(&self, i: usize) -> i64 { phera_extension_api::args::int(self.args, i).unwrap_or(0) }
    fn bytes(&self, i: usize) -> &[u8] { phera_extension_api::args::bytes(self.args, i) }
    fn object(&self) -> Result<&Rc<Object>, NativeError> { receiver(self.this) }
    fn native<T: Clone + 'static>(&self) -> Result<T, NativeError> {
        state(self.object()?, self.class)
    }
    fn result<T: 'static>(&mut self, value: T) -> NativeResult {
        if let Some(object) = self.this {
            store(object, value);
            Ok(Value::Object(object.clone()))
        } else { make(self.ctx, self.class, value) }
    }
    fn method(&self) -> bool { self.site.contains("::") }
}

fn assoc<const N: usize>(entries: [(&str, Value); N]) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {array.insert(phera_extension_api::ArrayKey::str(key), value);}
    array
}
