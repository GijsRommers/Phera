//! Generate executable Composer-style proxies: `vendor/bin/phpunit`.
//!
//! PHP proxies expose `_composer_autoload_path` and `_composer_bin_dir` globals.

use crate::{Package, Project, php, project::strings};
use anyhow::{Context, Result};
use std::fs;

pub(crate) fn write(project: &Project, packages: &[Package]) -> Result<()> {
    let vendor = project.vendor_dir()?;
    let bin_dir = project.bin_dir()?;
    for package in packages {
        for bin in strings(package.field("bin")) {
            let package_dir = vendor.join(&package.name);
            let source = crate::paths::inside(&package_dir, bin)?;
            if !source.is_file() {
                continue;
            }
            if !source
                .canonicalize()?
                .starts_with(package_dir.canonicalize()?)
            {
                continue;
            }
            fs::create_dir_all(&bin_dir)?;
            let name = source
                .file_name()
                .context("Binary path has no file name.")?;
            let destination = crate::paths::inside(&bin_dir, &name.to_string_lossy())?;
            let relative = php::relative(&bin_dir, &source);
            let bytes = fs::read(&source)?;
            let contents = String::from_utf8_lossy(&bytes);
            let after_shebang = if contents.starts_with("#!") {
                contents.split_once('\n').map_or("", |(_, rest)| rest)
            } else {
                &contents
            };
            let proxy = if after_shebang.trim_start().starts_with("<?php") {
                format!(
                    "#!/usr/bin/env php\n<?php\n\nnamespace Composer;\n\n\
                     $GLOBALS['_composer_bin_dir'] = __DIR__;\n\
                     $GLOBALS['_composer_autoload_path'] = __DIR__ . {};\n\
                     return include __DIR__ . {};\n",
                    php::quote(&format!(
                        "/{}",
                        php::relative(&bin_dir, &vendor.join("autoload.php"))
                    )),
                    php::quote(&format!("/{relative}")),
                )
            } else {
                let quoted = format!("'{}'", relative.replace('\'', "'\\''"));
                format!(
                    "#!/usr/bin/env sh\n\
                    dir=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n\
                    export COMPOSER_RUNTIME_BIN_DIR=\"$dir\"\n\
                    exec \"$dir\"/{quoted} \"$@\"\n"
                )
            };
            if !destination.exists() {
                fs::write(&destination, proxy)?;
            }
            crate::archive::set_mode(&destination, 0o755)?;
            crate::archive::set_mode(&source, 0o755)?;
        }
    }
    Ok(())
}
