//! Stage official framework templates and install their dependencies with Phera.

use super::Template;
use phera_packages::{Options, Project};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub(super) const SYMFONY_VERSION: &str = "7.4.*";
type Result = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn options() -> Options {
    Options {
        platform: phera_runtime::Runtime::new().platform_packages(),
        audit_summary: true,
        no_interaction: true,
        ..Options::default()
    }
}

pub(super) fn plain(directory: &Path) -> Result {
    Ok(phera_packages::install(directory, &options())?)
}

pub(super) fn framework(template: Template, directory: &Path, install: bool) -> Result {
    let parent = directory
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".phera-new-")
        .tempdir_in(parent)?;
    let stage = staging.path().join("project");
    fs::create_dir(&stage)?;
    let mut options = options();
    options.no_scripts = true;
    let (package, version) = match template {
        Template::Symfony | Template::SymfonyWebApp => ("symfony/skeleton", SYMFONY_VERSION),
        Template::Laravel => ("laravel/laravel", "*"),
        Template::Plain => return Err("A plain project does not use a framework template.".into()),
    };
    phera_packages::fetch_project(&stage, package, version, &options)?;
    if matches!(template, Template::Symfony | Template::SymfonyWebApp) {
        prepare_symfony(&stage, template == Template::SymfonyWebApp)?;
    }
    if install {
        phera_packages::install(&stage, &options)?;
    }
    super::refuse_existing_files(directory)?;
    fs::rename(stage, directory)?;
    Ok(())
}

pub(super) fn finish(directory: &Path) -> Result {
    let project = Project::read(directory)?;
    let options = options();
    for event in [
        "post-root-package-install",
        "post-autoload-dump",
        "post-install-cmd",
        "post-create-project-cmd",
    ] {
        if project.data["scripts"].get(event).is_some() {
            phera_packages::run_script(directory, event, &[], &options)?;
        }
    }
    Ok(())
}

fn prepare_symfony(directory: &Path, webapp: bool) -> Result {
    let mut manifest = Project::read(directory)?.data;
    let Some(requirements) = manifest["require"].as_object_mut() else {
        return Err("The Symfony template needs a require object.".into());
    };
    // Flex adds these concrete requirements when expanding the published skeleton.
    for component in ["console", "dotenv", "framework-bundle", "runtime", "yaml"] {
        requirements
            .entry(format!("symfony/{component}"))
            .or_insert(SYMFONY_VERSION.into());
    }
    if webapp {
        requirements
            .entry("symfony/webapp-pack")
            .or_insert(Value::from("*"));
    }
    let path = directory.join("composer.json");
    fs::write(path, serde_json::to_string_pretty(&manifest)? + "\n")?;
    Ok(())
}
