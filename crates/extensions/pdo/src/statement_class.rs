//! `PDO::ATTR_STATEMENT_CLASS => [MyStatement::class, [$arg]]` selects the PDOStatement subclass.

use std::rc::Rc;

use phera_extension_api::reflection::{ClassSelector, FunctionTarget, MemberKind};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value};

use crate::attributes::AttributeCall;

#[derive(Clone)]
pub struct StatementClass {
    pub name: Rc<str>,
    constructor: Option<Rc<str>>,
    arguments: Option<Rc<PhpArray>>,
}

impl Default for StatementClass {
    fn default() -> Self {
        Self {
            name: Rc::from("PDOStatement"),
            constructor: None,
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
        let Value::String(name) = &*class.deref() else {
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
                    result.constructor = Some(Rc::from(constructor.class.as_str()));
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
        if let Some(class) = &self.constructor {
            let reflection = context
                .reflection()
                .ok_or_else(|| NativeError::error("PDOStatement reflection is unavailable"))?;
            reflection
                .reflect_invoke(
                    FunctionTarget::Method {
                        class: class.to_string(),
                        name: "__construct".into(),
                    },
                    Some(object),
                    self.arguments.as_deref().cloned().unwrap_or_default(),
                )
                .await?;
        }
        Ok(())
    }
}
