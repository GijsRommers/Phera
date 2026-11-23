//! Filter options mix an integer bitmask with filter-specific options.
//!
//! ```php
//! filter_var('x', FILTER_VALIDATE_INT, ['options' => ['default' => 42]]); // 42
//! filter_var('1', FILTER_VALIDATE_INT, FILTER_FORCE_ARRAY); // [1]
//! ```

use phera_extension_api::value::{to_float, to_int};
use phera_extension_api::{ArrayKey, Context, NativeError, Param, ParamType, Value, prepare_call};

use crate::constants::*;

#[derive(Clone, Copy)]
pub enum Call {
    Var,
    Array,
    Input,
    InputArray,
}

impl Call {
    pub fn name(self) -> &'static str {
        match self {
            Self::Var => "filter_var",
            Self::Array => "filter_var_array",
            Self::Input => "filter_input",
            Self::InputArray => "filter_input_array",
        }
    }

    pub fn number(self) -> usize {
        match self {
            Self::Var => 3,
            Self::Input => 4,
            Self::Array | Self::InputArray => 2,
        }
    }

    pub fn option_error(self, condition: &str) -> NativeError {
        NativeError::value_error(format!(
            "{}(): Argument #{} ($options) {condition}",
            self.name(),
            self.number(),
        ))
    }

    /// The extension API has no array|int parameter. Apply its ordinary int
    /// coercion to the other branch, keeping the union's own diagnostics.
    pub fn union(self, context: &mut dyn Context, value: &Value) -> Result<Value, NativeError> {
        if matches!(value, Value::Int(_) | Value::Array(_)) {
            return Ok(value.clone());
        }
        if !context.strict_types() && value.is_null() {
            context.deprecated(format!(
                "{}(): Passing null to parameter #{} ($options) of type array|int is deprecated",
                self.name(),
                self.number(),
            ));
            return Ok(Value::Int(0));
        }
        let mut value = [value.clone()];
        let int = [Param::new("options", ParamType::Int)];
        match prepare_call(self.name(), &int, &mut value, context) {
            Ok(()) => Ok(std::mem::take(&mut value[0])),
            Err(_) => {
                let given = match &value[0] {
                    Value::Bool(true) => "true".into(),
                    Value::Bool(false) => "false".into(),
                    other => other.type_name(),
                };
                Err(NativeError::type_error(format!(
                    "{}(): Argument #{} ($options) must be of type array|int, {given} given",
                    self.name(),
                    self.number(),
                )))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    False,
    Null,
    Throw,
}

impl Failure {
    pub fn value(self) -> Value {
        match self {
            Self::False => Value::Bool(false),
            Self::Null | Self::Throw => Value::Null,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Flags(i64);

impl Flags {
    pub fn has(self, flag: i64) -> bool {
        self.0 & flag != 0
    }
}

pub struct Options {
    pub call: Call,
    pub id: i64,
    pub flags: Flags,
    pub require_scalar: bool,
    pub require_array: bool,
    pub force_array: bool,
    pub failure: Failure,
    pub data: Option<Value>,
}

impl Options {
    pub fn new(
        context: &mut dyn Context,
        call: Call,
        initial_id: i64,
        initial_flags: i64,
        definition: &Value,
    ) -> Result<Self, NativeError> {
        let mut id = initial_id;
        let mut flags = initial_flags;
        let mut data = None;
        match definition {
            Value::Array(definition) => {
                if let Some(filter) = definition.get(&ArrayKey::str("filter")) {
                    id = to_int(filter, context.diagnostics())?;
                }
                if let Some(option) = definition.get(&ArrayKey::str("options")) {
                    let option = option.deref();
                    if id == CALLBACK {
                        data = Some(option.clone());
                        flags = 0;
                    } else if option.is_array() {
                        data = Some(option.clone());
                    }
                }
                if let Some(bits) = definition.get(&ArrayKey::str("flags")) {
                    flags = scalar_flags(to_int(bits, context.diagnostics())?);
                }
            }
            value if initial_id == -1 => id = to_int(value, context.diagnostics())?,
            value => flags = scalar_flags(to_int(value, context.diagnostics())?),
        }
        let flags = Flags(flags);
        let failure = match (flags.has(NULL_ON_FAILURE), flags.has(THROW_ON_FAILURE)) {
            (true, true) => {
                return Err(call.option_error(
                    "cannot use both FILTER_NULL_ON_FAILURE and FILTER_THROW_ON_FAILURE",
                ));
            }
            (true, false) => Failure::Null,
            (false, true) => Failure::Throw,
            (false, false) => Failure::False,
        };
        Ok(Self {
            call,
            id,
            flags,
            failure,
            require_scalar: flags.has(REQUIRE_SCALAR),
            require_array: flags.has(REQUIRE_ARRAY),
            force_array: flags.has(FORCE_ARRAY),
            data,
        })
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        let Some(Value::Array(array)) = &self.data else {
            return None;
        };
        array.get(&ArrayKey::str(name))
    }

    pub fn string(&self, name: &str) -> Option<phera_extension_api::PhpString> {
        self.get(name)?.deref().to_php_string()
    }

    pub fn int(&self, context: &mut dyn Context, name: &str) -> Result<Option<i64>, NativeError> {
        self.get(name)
            .map(|value| to_int(value, context.diagnostics()).map_err(Into::into))
            .transpose()
    }

    pub fn float(&self, context: &mut dyn Context, name: &str) -> Result<Option<f64>, NativeError> {
        self.get(name)
            .map(|value| to_float(value, context.diagnostics()).map_err(Into::into))
            .transpose()
    }

    pub fn invalid(&self, message: &str) -> NativeError {
        NativeError::value_error(format!("{}(): {message}", self.call.name()))
    }
}

fn scalar_flags(flags: i64) -> i64 {
    if flags & (REQUIRE_ARRAY | FORCE_ARRAY) == 0 {
        flags | REQUIRE_SCALAR
    } else {
        flags
    }
}
