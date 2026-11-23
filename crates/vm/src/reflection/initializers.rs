//! `$attribute->getArguments()` loads code only for the arguments it reads.

use std::{collections::HashMap, sync::Arc};

use phera_bytecode::reflection::Initializer as SourceValue;
use phera_extension_api::{
    NativeError,
    reflection::{AttributeValue, Initializer},
};
use phera_value::Value;

use crate::{Vm, code_table::FunctionIndex};

#[derive(Default)]
pub(crate) struct InitializerCode(HashMap<(u32, usize), CompiledInitializer>);

#[derive(Clone)]
pub(super) enum CompiledInitializer {
    Function(FunctionIndex),
    Literal(Value),
}

pub(super) fn reflected_value(
    source: &SourceValue,
    unit: FunctionIndex,
    scope: Option<&str>,
) -> AttributeValue {
    let value = match source {
        SourceValue::Null => Value::Null,
        SourceValue::Bool(value) => Value::Bool(*value),
        SourceValue::Int(value) => Value::Int(*value),
        SourceValue::Float(value) => Value::Float(f64::from_bits(*value)),
        SourceValue::String(value) => Value::interned(value),
        SourceValue::EmptyArray => Value::from(phera_value::PhpArray::new()),
        SourceValue::Expression(id) => {
            return AttributeValue::Expression(Initializer {
                unit: unit.0,
                index: id.0,
                scope: scope.map(str::to_owned),
            });
        }
    };
    AttributeValue::Literal(value)
}

impl Vm {
    pub(super) async fn reflection_initializer_code(
        &mut self,
        initializer: &Initializer,
    ) -> Result<CompiledInitializer, NativeError> {
        let key = (initializer.unit, initializer.index);
        if let Some(compiled) = self.state.get_or_default::<InitializerCode>().0.get(&key) {
            return Ok(compiled.clone());
        }
        let unit = self
            .code
            .known_unit(FunctionIndex(initializer.unit))
            .cloned()
            .ok_or_else(|| NativeError::error("Reflection initializer's file was not loaded"))?;
        let index = initializer.index;
        let in_trait = unit
            .reflection_initializers
            .get(index)
            .and_then(|source| source.scope.as_deref())
            .and_then(|scope| self.classes.get(scope))
            .is_some_and(|class| class.kind == phera_bytecode::ClassKind::Trait);
        let compile = move || match (
            unit.reflection_source.as_deref(),
            unit.reflection_initializers.get(index),
        ) {
            (Some(source), Some(initializer)) => phera_compiler::compile_reflection_initializer(
                source,
                &unit.file,
                initializer,
                in_trait,
            )
            .map_err(|error| error.message),
            _ => Err("Reflection initializer has no source".into()),
        };
        let compiled = match tokio::runtime::Handle::try_current() {
            Ok(runtime) => runtime
                .spawn_blocking(compile)
                .await
                .map_err(|error| NativeError::error(error.to_string()))?,
            Err(_) => compile(),
        }
        .map_err(NativeError::error)?;
        let literal = compiled.class_constant.and_then(|(class, name)| {
            let class = self.classes.get(&class)?;
            let constant = class.constants.get(&name)?;
            if constant.deprecated.is_none()
                && constant.visibility == phera_bytecode::Visibility::Public
                && let crate::classes::class::ConstantInitializer::Value(value) =
                    &constant.initializer
                && matches!(
                    value,
                    Value::Null
                        | Value::Bool(_)
                        | Value::Int(_)
                        | Value::Float(_)
                        | Value::InternedStr(_)
                        | Value::String(_)
                )
            {
                Some(value.clone())
            } else {
                None
            }
        });
        let compiled = match literal {
            Some(value) => CompiledInitializer::Literal(value),
            None => CompiledInitializer::Function(self.code.load(&Arc::new(compiled.unit))),
        };
        self.state
            .get_or_default::<InitializerCode>()
            .0
            .insert(key, compiled.clone());
        Ok(compiled)
    }
}
