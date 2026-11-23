//! `PDO::ATTR_STATEMENT_CLASS => [MyStatement::class, [$arg]]` selects the PDOStatement subclass.

use std::rc::Rc;

use phera_extension_api::reflection::{ClassSelector, MemberKind};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value};

use crate::attributes::AttributeCall;

#[derive(Clone)]
pub struct StatementClass {
    pub name: Rc<str>,
    arguments: Option<Rc<PhpArray>>,
}

impl StatementClass {
    /// The copy for a fork of a snapshot: the constructor arguments may
    /// hold objects, which are the fork's own.
    pub fn forked(&self, fork: &mut phera_extension_api::value::Fork<'_>) -> Self {
        StatementClass {
            name: self.name.clone(),
            arguments: self
                .arguments
                .as_ref()
                .map(|arguments| fork.array(arguments)),
        }
    }
}

impl Default for StatementClass {
    fn default() -> Self {
        Self {
            name: Rc::from("PDOStatement"),
            arguments: None,
        }
    }
}

impl StatementClass {
    pub async fn parse(
        context: &mut dyn Context,
        value: &Value,
        call: AttributeCall,
    ) -> Result<Self, NativeError> {
        let argument = call.argument();
        let Value::Array(config) = &*value.deref() else {
            return Err(NativeError::type_error(format!(
                "{argument} PDO::ATTR_STATEMENT_CLASS value must be of type array, {} given",
                value.type_name()
            )));
        };
        let class = config.get(&ArrayKey::Int(0)).ok_or_else(|| {
            NativeError::value_error(format!(
                "{argument} PDO::ATTR_STATEMENT_CLASS value must be an array with the format \
                 array(classname, constructor_args)"
            ))
        })?;
        let invalid_class = || {
            NativeError::type_error(format!(
                "{argument} PDO::ATTR_STATEMENT_CLASS class must be a valid class"
            ))
        };
        let class = class.deref();
        let Some(name) = class.str_ref() else {
            return Err(invalid_class());
        };
        let name = name.to_string_lossy();
        let mut result = Self::default();
        if !name
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("PDOStatement")
        {
            let class = context
                .lookup_class(&name)
                .await?
                .ok_or_else(invalid_class)?;
            if !class.is_instance_of("PDOStatement") {
                return Err(NativeError::type_error(format!(
                    "{argument} PDO::ATTR_STATEMENT_CLASS class must be derived from PDOStatement"
                )));
            }
            result.name = Rc::from(class.name());
            if let Some(reflection) = context.reflection() {
                let methods = reflection
                    .reflect_members(ClassSelector::Name(class.name()), MemberKind::Method);
                if let Some(constructor) = methods
                    .iter()
                    .find(|member| member.name.eq_ignore_ascii_case("__construct"))
                {
                    const PUBLIC: i64 = 1;
                    if constructor.modifiers & PUBLIC != 0 {
                        return Err(NativeError::type_error(format!(
                            "{argument} User-supplied statement class cannot have \
                             a public constructor"
                        )));
                    }
                }
            }
        }
        if let Some(args) = config.get(&ArrayKey::Int(1)) {
            let Value::Array(args) = &*args.deref() else {
                return Err(NativeError::type_error(format!(
                    "{argument} PDO::ATTR_STATEMENT_CLASS constructor_args must be of type \
                     ?array, {} given",
                    args.type_name()
                )));
            };
            result.arguments = Some(args.clone());
        }
        Ok(result)
    }

    pub fn as_attribute(&self) -> NativeResult {
        let mut result = PhpArray::new();
        result.push(Value::from(self.name.as_ref()))?;
        if let Some(arguments) = &self.arguments {
            result.push(Value::Array(arguments.clone()))?;
        }
        Ok(Value::from(result))
    }

    pub async fn initialize(
        &self,
        context: &mut dyn Context,
        object: Rc<Object>,
    ) -> Result<(), NativeError> {
        let empty = PhpArray::new();
        context
            .call_hydration_constructor(object, self.arguments.as_deref().unwrap_or(&empty))
            .await?;
        Ok(())
    }
}
