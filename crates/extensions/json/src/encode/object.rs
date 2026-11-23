//! JSON object views. `json_encode($obj)` writes public properties unless
//! the object provides `JsonSerializable::jsonSerialize()`.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, Object, PhpArray, PropertyPurpose, Value,
};

use super::{Encoder, EncodingView, Error, Owner, Work};
use crate::recursion::Serializing;

impl Encoder {
    pub(super) async fn object(
        &mut self,
        context: &mut dyn Context,
        object: Rc<Object>,
        level: usize,
    ) -> Result<Option<EncodingView>, NativeError> {
        if object.class.is_instance_of("JsonSerializable") {
            let Some(guard) = Serializing::start(context, object.clone()) else {
                self.failure(Error::Recursion);
                return Ok(None);
            };
            let value = context
                .call_protocol_method(object.clone(), "jsonSerialize", Vec::new())
                .await?;
            if !matches!(&*value.deref(), Value::Object(result) if Rc::ptr_eq(&object, result)) {
                self.work.push(Work::ReleaseSerializer(guard));
                self.work.push(Work::value(value, level));
                return Ok(None);
            }
        } else if object.class.is_enum() {
            if let Some(value) = object.get(&ArrayKey::str("value")) {
                self.work.push(Work::value(value, level));
            } else {
                self.failure(Error::NonBackedEnum);
            }
            return Ok(None);
        }
        let identity = Rc::as_ptr(&object) as usize;
        if self.work.contains(identity) {
            self.failure(Error::Recursion);
            return Ok(None);
        }
        if context.property_view_can_suspend(&object, PropertyPurpose::Json) {
            return Ok(Some(EncodingView { object, level }));
        }
        let properties = if context.property_view_runs_code(&object, PropertyPurpose::Json) {
            match context
                .property_view(object.clone(), PropertyPurpose::Json, "json_encode", &[])
                .await?
            {
                phera_extension_api::PropertyView::Snapshot(properties)
                | phera_extension_api::PropertyView::DebugInfo(properties) => properties,
                phera_extension_api::PropertyView::DebugSlots { slots, .. } => slots,
                phera_extension_api::PropertyView::Recursive => {
                    self.failure(Error::Recursion);
                    return Ok(None);
                }
            }
        } else {
            Rc::new(context.object_properties(&object, PropertyPurpose::Json)?)
        };
        self.object_entries(object, level, properties);
        Ok(None)
    }

    pub(super) fn object_entries(
        &mut self,
        object: Rc<Object>,
        level: usize,
        properties: Rc<PhpArray>,
    ) {
        self.container(Owner::Object(object, properties), true, level);
    }
}
