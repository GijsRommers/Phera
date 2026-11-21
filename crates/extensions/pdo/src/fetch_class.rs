//! `fetchObject(Row::class)` hydrates columns before invoking Row's constructor.
//! `PDO::FETCH_PROPS_LATE` invokes the constructor before publishing columns.

use std::rc::Rc;

use phera_extension_api::reflection::{ClassSelector, MemberKind};
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value, args};

use crate::constants::{
    FETCH_CLASS, FETCH_CLASSTYPE, FETCH_GROUPING, FETCH_KNOWN_BITS, FETCH_MODE_BITS,
    FETCH_PROPS_LATE, FETCH_SERIALIZE,
};
use crate::fetch::{ValueOptions, stringify};
use crate::statement::{Statement, state};

#[derive(Clone, Copy)]
enum HydrationOrder {
    PropertiesFirst,
    ConstructorFirst,
}

#[derive(Clone)]
pub struct RowClass {
    name: String,
    arguments: Option<Rc<PhpArray>>,
    order: HydrationOrder,
}

impl RowClass {
    pub async fn configured(
        context: &mut dyn Context,
        arguments: &[Value],
        offset: usize,
        function: &str,
    ) -> Result<Option<Self>, NativeError> {
        let mode = args::int(arguments, offset).unwrap_or(0);
        if mode & FETCH_MODE_BITS != FETCH_CLASS {
            return Ok(None);
        }
        if mode < 0 || mode & !FETCH_KNOWN_BITS != 0 {
            let number = offset + 1;
            let name = if offset == 0 { "mode" } else { "fetchMode" };
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #{number} (${name}) must be a bitmask of PDO::FETCH_* constants"
            )));
        }
        if mode & (FETCH_CLASSTYPE | FETCH_SERIALIZE | FETCH_GROUPING) != 0 {
            return Err(crate::errors::unsupported("This PDO fetch class flag"));
        }
        let count = arguments.len();
        let minimum = offset + 2;
        let allow_default = function == "PDOStatement::fetchAll";
        if !allow_default && count < minimum || count > minimum + 1 {
            let relation = if count < minimum {
                "at least"
            } else {
                "at most"
            };
            let limit = if count < minimum {
                minimum
            } else {
                minimum + 1
            };
            return Err(NativeError::argument_count_error(format!(
                "{function}() expects {relation} {limit} arguments for the fetch mode provided, {count} given"
            )));
        }

        let order = if mode & FETCH_PROPS_LATE != 0 {
            HydrationOrder::ConstructorFirst
        } else {
            HydrationOrder::PropertiesFirst
        };
        let class = args::string(arguments, offset + 1);
        let class_name = class.map_or_else(|| "stdClass".into(), |name| name.to_string_lossy());
        let Some(class) = context.lookup_class(&class_name).await? else {
            return Err(NativeError::type_error(format!(
                "{function}(): Argument #{} must be a valid class",
                offset + 2
            )));
        };
        if class_name == "stdClass"
            && arguments
                .get(offset + 1)
                .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            return Err(NativeError::type_error(format!(
                "{function}(): Argument #{} must be a valid class",
                offset + 2
            )));
        }
        context.ensure_instantiable(class.name())?;
        let argument = args::value(arguments, offset + 2);
        let ctor_arguments = match &*argument.deref() {
            Value::Null => None,
            Value::Array(arguments) => Some(arguments.clone()),
            value => {
                return Err(NativeError::type_error(format!(
                    "{function}(): Argument #{} must be of type ?array, {} given",
                    offset + 3,
                    value.type_name()
                )));
            }
        };
        Ok(Some(Self {
            name: class.name().into(),
            arguments: ctor_arguments,
            order,
        }))
    }

    pub async fn object(
        context: &mut dyn Context,
        this: Option<&Rc<Object>>,
        arguments: &[Value],
    ) -> NativeResult {
        let name = args::string(arguments, 0)
            .map_or_else(|| "stdClass".into(), |name| name.to_string_lossy());
        let Some(class) = context.lookup_class(&name).await? else {
            return Err(NativeError::type_error(format!(
                "PDOStatement::fetchObject(): Argument #1 ($class) must be a valid class name, {name} given"
            )));
        };
        let arguments = args::array(arguments, 1).cloned();
        if arguments
            .as_ref()
            .is_some_and(|arguments| !arguments.is_empty())
            && let Some(reflection) = context.reflection()
            && !reflection
                .reflect_members(ClassSelector::Name(class.name()), MemberKind::Method)
                .iter()
                .any(|member| member.name.eq_ignore_ascii_case("__construct"))
        {
            return Err(NativeError::value_error(
                "PDOStatement::fetchObject(): Argument #2 ($constructorArgs) must be empty when class provided in argument #1 ($class) does not have a constructor",
            ));
        }
        context.ensure_instantiable(class.name())?;
        let class = Self {
            name: class.name().into(),
            arguments,
            order: HydrationOrder::PropertiesFirst,
        };
        class.next(context, this).await
    }

    pub async fn next(&self, context: &mut dyn Context, this: Option<&Rc<Object>>) -> NativeResult {
        let state = state(this)?;
        let fields = {
            let mut statement = state.borrow_mut();
            let Statement {
                rows,
                keys,
                cursor,
                connection,
                ..
            } = &mut *statement;
            let Some(row) = rows.values.get_mut(*cursor) else {
                return Ok(Value::Bool(false));
            };
            *cursor += 1;
            let attributes = ValueOptions::from(&*connection.attributes.borrow());
            let mut fields = PhpArray::with_capacity(row.len());
            for (value, key) in row.iter_mut().zip(keys.get(&rows.columns).keys) {
                fields.insert(
                    key.property.clone(),
                    stringify(context, std::mem::take(value), attributes)?,
                );
            }
            fields
        };
        self.hydrate(context, fields).await
    }

    async fn hydrate(&self, context: &mut dyn Context, fields: PhpArray) -> NativeResult {
        let (early, late) = match self.order {
            HydrationOrder::PropertiesFirst => (fields, None),
            HydrationOrder::ConstructorFirst => (PhpArray::new(), Some(fields)),
        };
        let Value::Object(object) = context.create_object(&self.name, early)? else {
            return Err(NativeError::error("PDO did not allocate a row object"));
        };
        let empty = PhpArray::new();
        context
            .call_hydration_constructor(object.clone(), self.arguments.as_deref().unwrap_or(&empty))
            .await?;
        if let Some(fields) = late {
            for (name, value) in fields.iter() {
                object.set(name, value.clone());
            }
        }
        Ok(Value::Object(object))
    }

    pub async fn all(&self, context: &mut dyn Context, this: Option<&Rc<Object>>) -> NativeResult {
        let remaining = {
            let state = state(this)?;
            let statement = state.borrow();
            statement.rows.values.len().saturating_sub(statement.cursor)
        };
        let mut rows = PhpArray::with_capacity(remaining);
        loop {
            let row = self.next(context, this).await?;
            if matches!(row, Value::Bool(false)) {
                return Ok(Value::from(rows));
            }
            rows.push(row)?;
        }
    }
}
