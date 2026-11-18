//! `unserialize('O:7:"Example":0:{}')` resolves `Example` before reading its properties.

use std::rc::Rc;

use phera_extension_api::serialization::{EnumCaseLookup, INCOMPLETE_CLASS, INCOMPLETE_CLASS_NAME};
use phera_extension_api::{ArrayKey, Context, NativeError, Object, Value};

use super::{BuildResult, Container, Decoder, Failure, emit_diagnostic, graph::Place};
use crate::wire::ObjectFormat;

impl Decoder<'_> {
    pub(super) async fn object(
        &mut self,
        context: &mut dyn Context,
        place: Place,
        class: &[u8],
        format: ObjectFormat,
    ) -> BuildResult<()> {
        let info = if self.options.allows(class) {
            if let Ok(name) = std::str::from_utf8(class) {
                match context.serialization_class(name).await? {
                    Some(class) => Some(class),
                    None => context.unserialize_class_callback(name).await?,
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(info) = &info
            && info.forbidden
        {
            return Err(NativeError::throw(
                "Exception",
                format!("Unserialization of '{}' is not allowed", info.name),
            )
            .into());
        }
        let name = info
            .as_ref()
            .map_or(INCOMPLETE_CLASS, |info| info.name.as_str());
        let custom = info.as_ref().is_some_and(|info| info.custom);
        match format {
            ObjectFormat::Custom => {
                let payload = self.reader.custom_data();
                emit_diagnostic(context, &mut self.reader);
                let payload = payload?;
                if !custom {
                    context.warning(format!("Class {name} has no unserializer"));
                }
                let object = allocate(context, name)?;
                self.graph.value(place, Value::Object(object.clone()));
                if custom {
                    context
                        .call_serialization_method(
                            object,
                            "unserialize",
                            vec![Value::from(payload)],
                        )
                        .await?;
                } else if info.is_none() {
                    object.set(ArrayKey::str(INCOMPLETE_CLASS_NAME), Value::from(class));
                }
            }
            ObjectFormat::Properties => {
                let count = self.reader.properties();
                emit_diagnostic(context, &mut self.reader);
                let count = count?;
                let has_unserialize = info.as_ref().is_some_and(|info| info.has_unserialize);
                if custom && !has_unserialize {
                    context.warning(format!("Erroneous data format for unserializing '{name}'"));
                    return Err(Failure::Parse);
                }
                let object = allocate(context, name)?;
                if info.is_none() {
                    object.set(ArrayKey::str(INCOMPLETE_CLASS_NAME), Value::from(class));
                }
                self.graph.value(place, Value::Object(object.clone()));
                let container = if has_unserialize {
                    Container::Unserialize(object)
                } else {
                    Container::Properties(object)
                };
                self.start_frame(context, count, container)?;
            }
        }
        Ok(())
    }

    pub(super) async fn enum_case(
        &mut self,
        context: &mut dyn Context,
        start: usize,
        name: &[u8],
    ) -> BuildResult<Value> {
        let Some(colon) = name.iter().position(|&byte| byte == b':') else {
            context.warning(format!(
                "unserialize(): Invalid enum name '{}' (missing colon)",
                diagnostic_name(name)
            ));
            return Err(Failure::At(start));
        };
        let class = &name[..colon];
        if !crate::wire::valid_class(class) {
            return Err(Failure::At(start));
        }
        let case = &name[colon + 1..];
        let lookup = match (std::str::from_utf8(class), std::str::from_utf8(case)) {
            (Ok(class), Ok(case)) => context.unserialize_enum_case(class, case).await?,
            (Ok(class), Err(_)) => match context.unserialize_enum_case(class, "").await? {
                EnumCaseLookup::MissingClass => EnumCaseLookup::MissingClass,
                EnumCaseLookup::NotEnum => EnumCaseLookup::NotEnum,
                _ => EnumCaseLookup::MissingCase,
            },
            _ => EnumCaseLookup::MissingClass,
        };
        let class = diagnostic_name(class);
        let case = diagnostic_name(case);
        let (warning, offset) = match lookup {
            EnumCaseLookup::Found(value) => return Ok(value),
            EnumCaseLookup::MissingClass => (format!("Class '{class}' not found"), start),
            EnumCaseLookup::NotEnum => (format!("Class '{class}' is not an enum"), start),
            EnumCaseLookup::MissingCase => (
                format!("Undefined constant {class}::{case}"),
                self.reader.offset(),
            ),
            EnumCaseLookup::NotCase => (
                format!("{class}::{case} is not an enum case"),
                self.reader.offset(),
            ),
        };
        context.warning(format!("unserialize(): {warning}"));
        Err(Failure::At(offset))
    }
}

fn allocate(context: &mut dyn Context, name: &str) -> BuildResult<Rc<Object>> {
    match context.create_unserialized_object(name)? {
        Value::Object(object) => Ok(object),
        _ => Err(NativeError::error("Unserialization requires a constructed object").into()),
    }
}

fn diagnostic_name(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.split(|&byte| byte == 0).next().unwrap_or_default()).into_owned()
}
