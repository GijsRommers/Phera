//! `var_dump`, `print_r` and `var_export`: one walk over a value, three
//! output styles.
//!
//! The walk keeps its own stack instead of recursing. PHP code can nest
//! arrays as deep as memory allows, and a recursive printer would overflow
//! the thread's stack on such a value and take the whole server down.

mod print_r;
mod var_dump;
mod var_export;

use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::value::Object;
use phera_extension_api::{
    ArrayKey, Context, DebugProperties, NativeError, PhpArray, Registry, Value,
};

pub(crate) fn register(registry: &mut Registry) {
    var_dump::register(registry);
    print_r::register(registry);
    var_export::register(registry);
}

/// An array or object whose entries are being printed.
pub(crate) enum Container {
    Array(Rc<PhpArray>),
    Object(Rc<Object>),
}

impl Container {
    /// Distinguishes this container from every other one alive, so a value
    /// that contains itself (through a reference, or an object property) is
    /// noticed. Two containers alive at the same time never share an address.
    fn identity(&self) -> usize {
        match self {
            Container::Array(array) => Rc::as_ptr(array) as usize,
            Container::Object(object) => Rc::as_ptr(object) as usize,
        }
    }
}

/// Where a value sits in the output.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    /// How many containers enclose it; 0 for the value being printed.
    pub depth: usize,
    /// Whether it is a PHP reference that something else still shares.
    pub is_reference: bool,
    pub leading_written: bool,
}

/// What differs between the three functions. The walker calls these in
/// document order.
pub(crate) trait Style {
    fn properties(&self) -> PropertyView<'_> {
        PropertyView::Raw
    }

    fn before_debug(&mut self, _out: &mut Vec<u8>, _place: Place, _context: &mut dyn Context) {}

    /// Anything that is not an array or object.
    fn scalar(&mut self, out: &mut Vec<u8>, value: &Value, place: Place);

    /// The start of an array or object with `len` entries.
    fn open(&mut self, out: &mut Vec<u8>, container: &Container, len: usize, place: Place);

    /// The key of the next entry. `depth` is the depth of the entry's value.
    fn key(&mut self, out: &mut Vec<u8>, key: &ArrayKey, container: &Container, depth: usize);

    /// After an entry's value, whatever it was.
    fn after_entry(&mut self, out: &mut Vec<u8>);

    /// The end of a container that was opened at `depth`.
    fn close(&mut self, out: &mut Vec<u8>, container: &Container, depth: usize);

    /// A container that is already being printed further up.
    fn recursion(
        &mut self,
        out: &mut Vec<u8>,
        container: &Container,
        place: Place,
        context: &mut dyn Context,
    );
}

/// Export uses raw properties. Inspection retains the native call site for errors.
pub(crate) enum PropertyView<'a> {
    Raw,
    Debug {
        function: &'static str,
        arguments: &'a [Value],
    },
}

/// A container whose entries are partly printed.
struct Frame {
    container: Container,
    /// The entries: the array itself, or a copy of an object's properties so
    /// no borrow of the object is held while printing.
    entries: Rc<PhpArray>,
    next: Option<usize>,
}

pub(crate) async fn walk(
    style: &mut dyn Style,
    out: &mut Vec<u8>,
    value: &Value,
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut active: HashSet<usize> = HashSet::new();
    frames.extend(enter(style, out, value, 0, &mut active, context).await?);

    loop {
        let depth = frames.len();
        let Some(frame) = frames.last_mut() else {
            break;
        };
        let entry = frame
            .next
            .and_then(|position| Some((position, frame.entries.entry_at(position)?)));
        match entry {
            Some((position, (key, item))) => {
                frame.next = frame.entries.next_position(position);
                style.key(out, &key, &frame.container, depth);
                match enter(style, out, item, depth, &mut active, context).await? {
                    Some(child) => frames.push(child),
                    None => style.after_entry(out),
                }
            }
            None => {
                style.close(out, &frame.container, depth - 1);
                active.remove(&frame.container.identity());
                frames.pop();
                if !frames.is_empty() {
                    style.after_entry(out);
                }
            }
        }
    }
    Ok(())
}

/// Prints a scalar, or the start of a container. Returns the frame to walk
/// when the value has entries to print.
async fn enter(
    style: &mut dyn Style,
    out: &mut Vec<u8>,
    value: &Value,
    depth: usize,
    active: &mut HashSet<usize>,
    context: &mut dyn Context,
) -> Result<Option<Frame>, NativeError> {
    // PHP marks a reference only while a second holder exists. A variable
    // that was the last one bound to it prints like a plain value.
    let is_reference = matches!(value, Value::Reference(cell) if Rc::strong_count(cell) > 1);
    let mut place = Place {
        depth,
        is_reference,
        leading_written: false,
    };
    let container = match &*value.deref() {
        Value::Array(array) => Container::Array(array.clone()),
        Value::Object(object) => Container::Object(object.clone()),
        scalar => {
            style.scalar(out, scalar, place);
            return Ok(None);
        }
    };
    if !active.insert(container.identity()) {
        style.recursion(out, &container, place, context);
        return Ok(None);
    }
    let entries = match &container {
        Container::Array(array) => array.clone(),
        Container::Object(object) => {
            if context.has_debug_info(object)
                && matches!(style.properties(), PropertyView::Debug { .. })
            {
                style.before_debug(out, place, context);
                place.leading_written = true;
                let PropertyView::Debug {
                    function,
                    arguments,
                } = style.properties()
                else {
                    return Err(NativeError::error("Invalid debug property view"));
                };
                match context
                    .debug_properties(object.clone(), function, arguments)
                    .await?
                {
                    DebugProperties::Properties(properties) => properties,
                    DebugProperties::Recursive => {
                        style.recursion(out, &container, place, context);
                        active.remove(&container.identity());
                        return Ok(None);
                    }
                }
            } else {
                Rc::new(object.to_array())
            }
        }
    };
    style.open(out, &container, entries.len(), place);
    Ok(Some(Frame {
        container,
        next: entries.first_position(),
        entries,
    }))
}

pub(crate) fn spaces(out: &mut Vec<u8>, count: usize) {
    out.resize(out.len() + count, b' ');
}

#[cfg(test)]
mod tests {
    use phera_extension_api::testing::TestContext;

    use super::*;

    /// Counts what the walker visits and prints nothing. The real styles
    /// indent by depth, so their output for a deep value grows with the
    /// square of the depth and cannot be built in a test.
    #[derive(Default)]
    struct Tally {
        opened: usize,
        closed: usize,
        scalars: usize,
    }

    impl Style for Tally {
        fn scalar(&mut self, _out: &mut Vec<u8>, _value: &Value, _place: Place) {
            self.scalars += 1;
        }

        fn open(&mut self, _out: &mut Vec<u8>, _container: &Container, _len: usize, _place: Place) {
            self.opened += 1;
        }

        fn key(
            &mut self,
            _out: &mut Vec<u8>,
            _key: &ArrayKey,
            _container: &Container,
            _depth: usize,
        ) {
        }

        fn after_entry(&mut self, _out: &mut Vec<u8>) {}

        fn close(&mut self, _out: &mut Vec<u8>, _container: &Container, _depth: usize) {
            self.closed += 1;
        }

        fn recursion(
            &mut self,
            _out: &mut Vec<u8>,
            _container: &Container,
            _place: Place,
            _context: &mut dyn Context,
        ) {
        }
    }

    /// A recursive walk would need one stack frame per level and overflow
    /// long before 200,000 levels.
    #[test]
    fn walking_a_deeply_nested_array_does_not_use_the_stack() {
        let mut value = Value::Int(1);
        for _ in 0..200_000 {
            let mut array = PhpArray::new();
            array.insert(ArrayKey::int(0), value);
            value = Value::from(array);
        }
        let mut tally = Tally::default();
        {
            use std::{
                future::Future,
                task::{Context as TaskContext, Poll, Waker},
            };
            let mut out = Vec::new();
            let mut context = TestContext::default();
            let mut walk = std::pin::pin!(walk(&mut tally, &mut out, &value, &mut context));
            assert!(matches!(
                walk.as_mut()
                    .poll(&mut TaskContext::from_waker(Waker::noop())),
                Poll::Ready(Ok(()))
            ));
        }
        assert_eq!(
            (tally.opened, tally.closed, tally.scalars),
            (200_000, 200_000, 1)
        );
    }
}
