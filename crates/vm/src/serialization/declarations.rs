//! PHP validates serialization method signatures when it declares the class.
//!
//! `class Example { public function __unserialize(array $data): void {} }`

use phera_bytecode::{ClassKind, TypeMask, Visibility};
use phera_extension_api::{NativeError, Unwind};

use crate::Vm;
use crate::classes::{Class, Method, MethodBody};
use crate::diagnostics::SourceLocation;
use crate::error_traces::DeclarationFailure;

impl Vm {
    pub(crate) fn check_serialization_method(
        &mut self,
        lower_name: &str,
        method: &Method,
    ) -> Result<(), NativeError> {
        let (parameters, result) = match lower_name {
            "__serialize" | "__sleep" => (0, TypeMask::ARRAY),
            "__unserialize" => (1, TypeMask::VOID),
            "__wakeup" => (0, TypeMask::VOID),
            _ => return Ok(()),
        };
        let MethodBody::User(function) = method.body else {
            return Ok(());
        };
        let body = self.code.function(function);
        let unit = self.code.unit(function);
        let location = SourceLocation {
            file: unit.file.clone(),
            line: body.definition_line as usize,
        };
        let name = format!("{}::{}()", method.declaring, method.name);
        let failure = |message| {
            NativeError::Unwind(Unwind(Box::new(DeclarationFailure {
                message,
                file: location.file.clone(),
                line: location.line,
            })))
        };
        if body.parameters.len() != parameters {
            let phrase = if parameters == 0 {
                "cannot take arguments"
            } else {
                "must take exactly 1 argument"
            };
            return Err(failure(format!("Method {name} {phrase}")));
        }
        if body
            .parameters
            .iter()
            .any(|parameter| parameter.by_reference)
        {
            return Err(failure(format!(
                "Method {name} cannot take arguments by reference"
            )));
        }
        if method.is_static {
            return Err(failure(format!("Method {name} cannot be static")));
        }
        if let Some(parameter) = body.parameters.first()
            && let Some(ty) = parameter.ty.and_then(|ty| unit.types.get(ty.index()))
            && !ty.mask.intersects(TypeMask::ARRAY)
        {
            let parameter = &unit.names[parameter.name.index()];
            return Err(failure(format!(
                "{name}: Parameter #1 (${parameter}) must be of type array when declared"
            )));
        }
        if let Some(ty) = body.return_type.and_then(|ty| unit.types.get(ty.index()))
            && ty.mask != result
            && ty.mask != TypeMask::NEVER
        {
            let result = if result == TypeMask::ARRAY {
                "array"
            } else {
                "void"
            };
            return Err(failure(format!(
                "{name}: Return type must be {result} when declared"
            )));
        }
        if method.visibility != Visibility::Public {
            let previous = self.diagnostic_location.replace(location);
            self.warn(format!(
                "The magic method {name} must have public visibility"
            ));
            self.diagnostic_location = previous;
        }
        Ok(())
    }

    pub(crate) fn deprecate_legacy_serializable(
        &mut self,
        class: &Class,
        location: SourceLocation,
    ) {
        if class.kind == ClassKind::Class
            && !class.is_abstract
            && class.is_a("Serializable")
            && (Self::serialization_method(class, "__serialize").is_none()
                || Self::serialization_method(class, "__unserialize").is_none())
        {
            let previous = self.diagnostic_location.replace(location);
            self.diagnostic(
                "Deprecated",
                format!(
                    "{} implements the Serializable interface, which is deprecated. Implement \
                 __serialize() and __unserialize() instead (or in addition, if support for old \
                 PHP versions is necessary)",
                    class.name
                ),
            );
            self.diagnostic_location = previous;
        }
    }
}
