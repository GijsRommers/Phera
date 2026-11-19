//! PHP iterator instructions instruction handlers, separate from dispatch.

use phera_bytecode::{IterId, PathId, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Implements PHP IterStartReference semantics outside the dispatch loop.
    pub(crate) fn start_reference_iterator(
        &mut self,
        iterator: IterId,
        path: PathId,
        subject: Reg,
    ) -> Result<bool, NativeError> {
        let place = self.path(path)?;
        let value = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        if let phera_value::Value::Object(ref object) = value {
            if object.class.is_traversable() {
                self.set(subject, value);
                return Ok(false);
            }
            if let Some(class) = self.hooked_property_class(object) {
                self.start_hooked_property_iteration(
                    iterator,
                    object.clone(),
                    class,
                    crate::access_mode::Binding::ByReference,
                );
                return Ok(true);
            }
        }
        drop(value);
        let reference = self.reference(&place)?;
        self.set(subject, reference);
        self.iter_start_reference(iterator, crate::AccessPath::local(subject))?;
        Ok(true)
    }

    /// A property getter may suspend before foreach receives its subject.
    pub(crate) async fn finish_reference_foreach(
        &mut self,
        iterator: IterId,
        subject: Reg,
        value: Value,
    ) -> Result<(), NativeError> {
        let receiver = match &*value.deref() {
            Value::Object(object) => Some(object.clone()),
            _ => None,
        };
        if let Some(object) = receiver {
            if object.class.is_traversable() {
                self.set(subject, Value::Null);
                return self
                    .start_traversable(iterator, Value::Object(object), true)
                    .await;
            }
            if let Some(class) = self.hooked_property_class(&object) {
                self.start_hooked_property_iteration(
                    iterator,
                    object,
                    class,
                    crate::access_mode::Binding::ByReference,
                );
                return Ok(());
            }
        }
        self.set(subject, value);
        self.iter_start_reference(iterator, crate::AccessPath::local(subject))
    }

    /// Implements PHP IterEnd semantics outside the dispatch loop.
    pub(crate) fn end_iterator(&mut self, iterator: IterId) -> Result<(), NativeError> {
        let state = self
            .call_stack
            .current_extras_mut()
            .iterators
            .remove(&iterator.0);
        if let Some(mut state) = state {
            self.release_iterator_subject(&mut state)?;
        }
        Ok(())
    }
}
