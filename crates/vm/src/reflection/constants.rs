//! `new ReflectionConstant('PHP_VERSION')` reads the registry or request constant table.

use phera_extension_api::NativeError;
use phera_extension_api::reflection::{AttributeTarget, ConstantInfo, SourceInfo};

use crate::{Vm, constant_table::ConstantOrigin};

use super::{exception, source_info};

impl Vm {
    pub(super) fn reflection_constant(&self, name: &str) -> Result<ConstantInfo, NativeError> {
        let canonical = name.strip_prefix('\\').unwrap_or(name);
        let constant = self
            .constants
            .get(canonical)
            .ok_or_else(|| exception(format!("Constant \"{name}\" does not exist")))?;
        let mut attributes = Vec::new();
        let source = match self.constants.origin(canonical) {
            Some(ConstantOrigin::Declaration(function)) => {
                let unit = self.code.unit(function);
                unit.constant_metadata.get(canonical).map(|metadata| {
                    attributes = self.reflection_attributes(
                        function,
                        &metadata.attributes,
                        AttributeTarget::Constant,
                        None,
                    );
                    source_info(unit, metadata)
                })
            }
            Some(ConstantOrigin::Define(function)) => Some(SourceInfo {
                file: self.code.unit(function).file.clone(),
                ..SourceInfo::default()
            }),
            None => None,
        };
        let extension = self
            .registry
            .constant_names()
            .find(|(name, _)| *name == canonical)
            .map(|(_, module)| module.to_owned());
        Ok(ConstantInfo {
            name: canonical.into(),
            source,
            extension,
            deprecated: constant.deprecated.is_some(),
            attributes,
        })
    }
}
