//! `unserialize('E:9:"Color:Red";') === Color::Red`.

use phera_bytecode::ClassKind;
use phera_extension_api::NativeError;
use phera_extension_api::serialization::EnumCaseLookup;

use crate::Vm;
use crate::classes::class::ConstantInitializer;

impl Vm {
    pub(crate) async fn lookup_unserialized_enum(
        &mut self,
        class: &str,
        case: &str,
    ) -> Result<EnumCaseLookup, NativeError> {
        let Some(class) = self.lookup_class(class).await? else {
            return Ok(EnumCaseLookup::MissingClass);
        };
        if class.kind != ClassKind::Enum {
            return Ok(EnumCaseLookup::NotEnum);
        }
        let Some(constant) = class.constants.get(case) else {
            return Ok(EnumCaseLookup::MissingCase);
        };
        if !matches!(constant.initializer, ConstantInitializer::EnumCase { .. }) {
            return Ok(EnumCaseLookup::NotCase);
        }
        self.class_constant(&class.name, case)
            .map(EnumCaseLookup::Found)
    }
}
