//! Report Composer requirements such as `"require": {"ext-json": "*"}` from the
//! PHP modules registered by Phera.

use crate::Runtime;
use phera_extension_api::ConstantValue;
use std::collections::BTreeMap;

impl Runtime {
    /// Composer platform packages supplied by this runtime.
    pub fn platform_packages(&self) -> BTreeMap<String, String> {
        let mut packages = BTreeMap::new();
        let Some(ConstantValue::String(version)) = self.registry.get_constant("PHP_VERSION") else {
            return packages;
        };
        packages.insert("php".into(), (*version).into());
        if cfg!(target_pointer_width = "64") {
            packages.insert("php-64bit".into(), (*version).into());
        }
        for extension in self.registry.extensions() {
            let name = extension.to_ascii_lowercase().replace(' ', "-");
            let extension_version = self.registry.module_version(extension).unwrap_or(version);
            packages.insert(format!("ext-{name}"), extension_version.into());
        }
        for (constant, package) in [
            ("PCRE_VERSION", "lib-pcre"),
            ("PCRE_UNICODE_VERSION", "lib-pcre-unicode"),
        ] {
            if let Some(ConstantValue::String(version)) = self.registry.get_constant(constant)
                && let Some(version) = version.split_whitespace().next()
            {
                packages.insert(package.into(), version.into());
            }
        }
        packages
    }
}
