//! Write relocatable Composer PHP maps and bootstrap files.
//!
//! After `require 'vendor/autoload.php'`, PHP uses the bundled Composer ClassLoader.

use super::Maps;
use crate::{Lock, Options, Project, php};
use anyhow::Result;
use md5::{Digest, Md5};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn write(project: &Project, maps: &Maps, options: &Options) -> Result<()> {
    let vendor = project.vendor_dir()?;
    let composer = vendor.join("composer");
    fs::write(
        composer.join("ClassLoader.php"),
        include_bytes!("../../resources/ClassLoader.php"),
    )?;
    fs::write(
        composer.join("InstalledVersions.php"),
        include_bytes!("../../resources/InstalledVersions.php"),
    )?;
    fs::write(
        composer.join("LICENSE"),
        concat!("\n", include_str!("../../resources/LICENSE.composer"), "\n"),
    )?;
    for (filename, mappings) in [
        ("autoload_psr4.php", &maps.psr4),
        ("autoload_namespaces.php", &maps.psr0),
    ] {
        let mut code = php::header(&vendor, &project.directory, filename);
        code.push_str("return array(\n");
        for (prefix, paths) in mappings.iter().rev() {
            let paths = paths
                .iter()
                .map(|path| php::path(path, &vendor, &project.directory))
                .collect::<Vec<_>>()
                .join(", ");
            code.push_str(&format!("    {} => array({paths}),\n", php::quote(prefix)));
        }
        code.push_str(");\n");
        fs::write(composer.join(filename), code)?;
    }
    for (filename, mappings) in [
        (
            "autoload_classmap.php",
            maps.classes
                .iter()
                .map(|(name, path)| (name.as_slice(), path))
                .collect::<Vec<_>>(),
        ),
        (
            "autoload_files.php",
            maps.files
                .iter()
                .map(|(name, path)| (name.as_bytes(), path))
                .collect(),
        ),
    ] {
        if filename == "autoload_files.php" && mappings.is_empty() {
            remove_optional(&composer.join(filename))?;
            continue;
        }
        let mut code = php::header(&vendor, &project.directory, filename);
        code.push_str("return array(\n");
        for (name, path) in mappings {
            code.push_str(&format!(
                "    {} => {},\n",
                php::quote_bytes(name),
                php::path(path, &vendor, &project.directory)
            ));
        }
        code.push_str(");\n");
        fs::write(composer.join(filename), code)?;
    }
    let suffix = loader_suffix(project, &vendor)?;
    let static_code = super::static_map::render(maps, &vendor, &project.directory, &suffix);
    fs::write(composer.join("autoload_static.php"), static_code)?;
    let authoritative = options.authoritative(project);
    let include_paths = write_include_paths(project, &vendor, &maps.include_paths)?;
    let check_platform = if super::platform::write(project, options)? {
        "        require __DIR__ . '/platform_check.php';\n\n"
    } else {
        ""
    };
    let real = include_str!("../../resources/autoload_real.php.in")
        .replace("@INCLUDE_PATHS@", include_paths)
        .replace("@PLATFORM_CHECK@", check_platform)
        .replace(
            "@AUTHORITATIVE@",
            if authoritative {
                "        $loader->setClassMapAuthoritative(true);\n"
            } else {
                ""
            },
        )
        .replace(
            "@FILES@",
            if maps.files.is_empty() {
                ""
            } else {
                concat!(include_str!("../../resources/autoload_files.php.in"), "\n")
            },
        )
        .replace(
            "@PREPEND@",
            if project.data["config"]["prepend-autoloader"] == false {
                "false"
            } else {
                "true"
            },
        )
        .replace(
            "@USE_INCLUDE_PATH@",
            if project.config_bool("use-include-path") {
                "        $loader->setUseIncludePath(true);\n"
            } else {
                ""
            },
        )
        .replace("@SUFFIX@", &suffix);
    fs::write(composer.join("autoload_real.php"), real)?;
    let autoload = include_str!("../../resources/autoload.php.in").replace("@SUFFIX@", &suffix);
    fs::write(vendor.join("autoload.php"), autoload)?;
    if options.verbose {
        eprintln!("Generated autoloader: {} classes.", maps.classes.len());
    }
    Ok(())
}

pub(super) fn remove_optional(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn loader_suffix(project: &Project, vendor: &Path) -> Result<String> {
    let valid = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    };
    if let Some(configured) = project.data["config"]["autoloader-suffix"].as_str()
        && !configured.is_empty()
    {
        anyhow::ensure!(
            valid(configured),
            "config.autoloader-suffix must be a PHP identifier suffix."
        );
        return Ok(configured.to_owned());
    }
    if let Ok(existing) = fs::read_to_string(vendor.join("autoload.php"))
        && let Some(initializer) = existing.split_once("return ComposerAutoloaderInit")
        && let Some((suffix, _)) = initializer.1.split_once("::getLoader(")
        && valid(suffix)
    {
        return Ok(suffix.to_owned());
    }
    let hash = Lock::read(&project.directory)?.content_hash;
    if !hash.is_empty()
        && hash
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Ok(hash);
    }
    Ok(base16ct::lower::encode_string(&Md5::digest(fs::read(
        project.directory.join("composer.lock"),
    )?)))
}

fn write_include_paths(
    project: &Project,
    vendor: &Path,
    paths: &[PathBuf],
) -> Result<&'static str> {
    let filename = "include_paths.php";
    let destination = vendor.join("composer").join(filename);
    if paths.is_empty() {
        remove_optional(&destination)?;
        return Ok("");
    }
    let mut code = php::header(vendor, &project.directory, filename);
    code.push_str("return array(\n");
    for path in paths {
        let expression = php::path(path, vendor, &project.directory);
        code.push_str(&format!("    {expression},\n"));
    }
    code.push_str(");\n");
    fs::write(destination, code)?;
    Ok(concat!(
        "        $includePaths = require __DIR__ . '/include_paths.php';\n",
        "        $includePaths[] = get_include_path();\n",
        "        set_include_path(implode(PATH_SEPARATOR, $includePaths));\n\n",
    ))
}
