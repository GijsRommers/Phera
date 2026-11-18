//! `serialize($item)` can copy linked wire keys for an unmodified scalar layout.

use phera_extension_api::{Context, NativeError, Object, Value};

use super::super::{Encoder, write_scalar};

impl Encoder {
    pub(in crate::encode) fn write_plain_object(
        &mut self,
        context: &mut dyn Context,
        object: &Object,
    ) -> Result<bool, NativeError> {
        let Some(fields) = context.plain_serialization_fields(object) else {
            return Ok(false);
        };
        object
            .with_properties(|properties| {
                if properties.len() != fields.len()
                    || !properties
                        .iter()
                        .zip(fields.iter())
                        .all(|((key, value), field)| {
                            key == field.key
                                && !matches!(
                                    value,
                                    Value::Array(_) | Value::Object(_) | Value::Reference(_)
                                )
                        })
                {
                    return Ok(false);
                }
                self.output
                    .object(object.class.name().as_bytes(), fields.len());
                for (value, field) in properties.values().zip(fields.iter()) {
                    self.output.property(&field.token);
                    if !write_scalar(&mut self.output, &mut self.precision, context, value) {
                        return Err(NativeError::error("Invalid scalar property table"));
                    }
                }
                self.slots.count += fields.len();
                self.output.close();
                Ok(true)
            })
            .unwrap_or(Ok(false))
    }
}
