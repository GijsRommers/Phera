//! The dump walker can hold its place while `var_export($object)` reads a hook.

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::{ArrayKey, Context, NativeError, Object, PhpArray, PropertyView, Value};

use super::{Container, Place, Style};

/// A container whose entries are partly printed.
struct Frame {
    container: Container,
    /// The entries: the array itself, or a copy of an object's properties so
    /// no borrow of the object is held while printing.
    entries: Rc<PhpArray>,
    source: EntrySource,
    uninitialized: Option<std::collections::HashMap<ArrayKey, String>>,
    next: Option<usize>,
}

#[derive(Clone, Copy)]
enum EntrySource {
    Live,
    PropertySnapshot,
}

impl Frame {
    /// Copying an object's property table adds a Rust owner, not a PHP alias.
    fn duplicate_owner(&self, key: &ArrayKey, value: &Value) -> usize {
        if let (EntrySource::PropertySnapshot, Container::Object(object), Value::Reference(cell)) =
            (self.source, &self.container, value)
            && let Some(Value::Reference(original)) = object.get(key)
        {
            return usize::from(Rc::ptr_eq(&original, cell));
        }
        0
    }
}

pub(super) struct WalkView {
    pub(super) object: Rc<Object>,
    place: Place,
}

enum Entered {
    Scalar,
    Container(Frame),
    View(WalkView),
}

#[derive(Default)]
pub(super) struct Walker {
    frames: Vec<Frame>,
    active: HashSet<usize>,
}

pub(super) async fn walk(
    style: &mut dyn Style,
    out: &mut Vec<u8>,
    value: &Value,
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    match Walker::default()
        .advance(style, out, Some(value), context)
        .await?
    {
        Some(_) => Err(NativeError::error(
            "Phera internal error: suspended debug walk",
        )),
        None => Ok(()),
    }
}

impl Walker {
    pub(super) fn resume(
        &mut self,
        style: &mut dyn Style,
        out: &mut Vec<u8>,
        view: WalkView,
        properties: Rc<PhpArray>,
    ) {
        self.frames.push(open_frame(
            style,
            out,
            Container::Object(view.object),
            view.place,
            properties,
            EntrySource::PropertySnapshot,
            None,
        ));
    }

    pub(super) async fn advance(
        &mut self,
        style: &mut dyn Style,
        out: &mut Vec<u8>,
        root: Option<&Value>,
        context: &mut dyn Context,
    ) -> Result<Option<WalkView>, NativeError> {
        if let Some(root) = root {
            match enter(style, out, root, 0, 0, &mut self.active, context).await? {
                Entered::Scalar => {}
                Entered::Container(frame) => self.frames.push(frame),
                Entered::View(view) => return Ok(Some(view)),
            }
        }
        loop {
            let depth = self.frames.len();
            let Some(frame) = self.frames.last_mut() else {
                return Ok(None);
            };
            let entry = frame
                .next
                .and_then(|position| Some((position, frame.entries.entry_at(position)?)));
            match entry {
                Some((position, (key, item))) => {
                    frame.next = frame.entries.next_position(position);
                    style.key(out, &key, &frame.container, depth);
                    if let Some(ty) = frame
                        .uninitialized
                        .as_ref()
                        .and_then(|slots| slots.get(&key))
                    {
                        style.uninitialized(out, ty, depth);
                        style.after_entry(out);
                        continue;
                    }
                    let duplicate = frame.duplicate_owner(&key, item);
                    match enter(
                        style,
                        out,
                        item,
                        depth,
                        duplicate,
                        &mut self.active,
                        context,
                    )
                    .await?
                    {
                        Entered::Scalar => style.after_entry(out),
                        Entered::Container(child) => self.frames.push(child),
                        Entered::View(view) => return Ok(Some(view)),
                    }
                }
                None => {
                    style.close(out, &frame.container, depth - 1);
                    self.active.remove(&frame.container.identity());
                    self.frames.pop();
                    if !self.frames.is_empty() {
                        style.after_entry(out);
                    }
                }
            }
        }
    }
}

/// Prints a scalar, or the start of a container. Returns the frame to walk
/// when the value has entries to print.
async fn enter(
    style: &mut dyn Style,
    out: &mut Vec<u8>,
    value: &Value,
    depth: usize,
    duplicate_owner: usize,
    active: &mut HashSet<usize>,
    context: &mut dyn Context,
) -> Result<Entered, NativeError> {
    // PHP marks a reference only while a second holder exists. A variable
    // that was the last one bound to it prints like a plain value.
    let is_reference =
        matches!(value, Value::Reference(cell) if Rc::strong_count(cell) > 1 + duplicate_owner);
    let mut place = Place {
        depth,
        is_reference,
        leading_written: false,
    };
    let container = match &*value.deref() {
        Value::Array(array) => Container::Array(array.clone()),
        Value::Object(object)
            if object.class.is_enum()
                && let Some(Value::String(name)) = object.get(&ArrayKey::str("name"))
                && style.enum_case(out, object, &name, place) =>
        {
            return Ok(Entered::Scalar);
        }
        Value::Object(object) => Container::Object(object.clone()),
        scalar => {
            style.scalar(out, scalar, place, context);
            return Ok(Entered::Scalar);
        }
    };
    if !active.insert(container.identity()) {
        style.recursion(out, &container, place, context);
        return Ok(Entered::Scalar);
    }
    let (entries, source, uninitialized) = match &container {
        Container::Array(array) => (array.clone(), EntrySource::Live, None),
        Container::Object(object) => {
            let purpose = style.property_purpose();
            if context.property_view_runs_code(object, purpose) {
                style.before_debug(out, place, context);
                place.leading_written = true;
            }
            if context.property_view_can_suspend(object, purpose) {
                return Ok(Entered::View(WalkView {
                    object: object.clone(),
                    place,
                }));
            }
            let (function, arguments) = style.call_site();
            match context
                .property_view(object.clone(), purpose, function, arguments)
                .await?
            {
                PropertyView::Snapshot(properties) => {
                    (properties, EntrySource::PropertySnapshot, None)
                }
                PropertyView::DebugInfo(properties) => (properties, EntrySource::Live, None),
                PropertyView::DebugSlots {
                    slots,
                    uninitialized,
                } => (slots, EntrySource::PropertySnapshot, Some(uninitialized)),
                PropertyView::Recursive => {
                    style.recursion(out, &container, place, context);
                    active.remove(&container.identity());
                    return Ok(Entered::Scalar);
                }
            }
        }
    };
    Ok(Entered::Container(open_frame(
        style,
        out,
        container,
        place,
        entries,
        source,
        uninitialized,
    )))
}

fn open_frame(
    style: &mut dyn Style,
    out: &mut Vec<u8>,
    container: Container,
    place: Place,
    entries: Rc<PhpArray>,
    source: EntrySource,
    uninitialized: Option<std::collections::HashMap<ArrayKey, String>>,
) -> Frame {
    let initialized = entries
        .len()
        .saturating_sub(uninitialized.as_ref().map_or(0, |slots| slots.len()));
    style.open(out, &container, initialized, place);
    Frame {
        container,
        next: entries.first_position(),
        entries,
        source,
        uninitialized,
    }
}
