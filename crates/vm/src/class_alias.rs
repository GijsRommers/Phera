//! `class_alias(Original::class, 'Other')` adds a second name for the same class identity.

use phera_extension_api::NativeResult;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// An alias never duplicates code, metadata, or static property storage.
    pub(crate) async fn class_alias(
        &mut self,
        name: &str,
        alias: &str,
        autoload: bool,
        arguments: &[Value],
    ) -> NativeResult {
        if self
            .state
            .get::<phera_extension_api::ImageSeal>()
            .is_some_and(|seal| seal.forbid_class_alias)
        {
            return Err(self.sealed_class_alias_error());
        }
        let class = if autoload {
            self.lookup_class(name).await?
        } else {
            self.classes.get(name)
        };
        let Some(class) = class else {
            self.warn(format!("Class \"{name}\" not found"));
            return Ok(Value::Bool(false));
        };
        let key = alias
            .strip_prefix('\\')
            .unwrap_or(alias)
            .to_ascii_lowercase();
        if let Some(short_name) = key.rsplit('\\').next()
            && matches!(
                short_name,
                "array"
                    | "bool"
                    | "callable"
                    | "false"
                    | "float"
                    | "int"
                    | "iterable"
                    | "mixed"
                    | "never"
                    | "null"
                    | "object"
                    | "parent"
                    | "self"
                    | "static"
                    | "string"
                    | "true"
                    | "void"
            )
        {
            return Err(self.fatal_native_error(
                64,
                format!("Cannot use \"{key}\" as a class alias as it is reserved"),
                "class_alias",
                arguments,
            ));
        }
        if key == "_" {
            return Err(crate::function_introspection::deprecated(
                "Using \"_\" as a class alias",
                "8.4",
            ));
        }
        if self.classes.alias(key, class.clone()) {
            return Ok(Value::Bool(true));
        }
        let message = self.class_redeclaration_message(&class, alias);
        self.warn(message);
        Ok(Value::Bool(false))
    }

    pub(crate) fn class_redeclaration_message(
        &self,
        class: &crate::classes::Class,
        name: &str,
    ) -> String {
        let kind = class.kind.description().to_ascii_lowercase();
        let mut message = format!("Cannot redeclare {kind} {name}");
        if let Some((function, index)) = class.declaration {
            let unit = self.code.unit(function);
            if let Some(declaration) = unit.classes.get(index.index()) {
                let (file, line) = (&unit.file, declaration.line);
                message.push_str(&format!(" (previously declared in {file}:{line})"));
            }
        }
        message
    }

    /// Membership uses canonical names on a miss so aliases work for ancestors as well.
    pub(crate) fn class_matches(&self, class: &crate::classes::Class, name: &str) -> bool {
        class.is_a(name) || self.alias_matches(class, name)
    }

    #[cold]
    pub(crate) fn alias_matches(&self, class: &crate::classes::Class, name: &str) -> bool {
        self.classes
            .get(name)
            .is_some_and(|target| class.is_a(&target.name))
    }

    #[cold]
    fn sealed_class_alias_error(&self) -> phera_extension_api::NativeError {
        const SEALED: &str = "Phera\\SealedImageException";
        let message = "class_alias() is disabled in strict AOT image mode";
        if self.classes.get_ref(SEALED).is_some() {
            phera_extension_api::NativeError::throw(SEALED, message)
        } else {
            phera_extension_api::NativeError::error(message)
        }
    }
}
