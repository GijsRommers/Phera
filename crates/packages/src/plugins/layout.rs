//! Framework locations from `extra.installer-paths` and `extra.wordpress-install-dir`.
//!
//! A Drupal project's `"web/core": ["type:drupal-core"]` moves core out of vendor.

use crate::{Package, Project, project::strings};

pub(crate) const INSTALLERS: &str = "composer/installers";
pub(crate) const WORDPRESS_CORE: &str = "roots/wordpress-core-installer";

pub(crate) fn custom(
    project: &Project,
    package: &Package,
    installers: bool,
    wordpress_core: bool,
) -> Option<String> {
    let kind = package.kind();
    if wordpress_core && kind == "wordpress-core" {
        let configured = &project.data["extra"]["wordpress-install-dir"];
        return configured
            .as_str()
            .or_else(|| configured[&package.name].as_str())
            .or_else(|| package.field("extra")["wordpress-install-dir"].as_str())
            .filter(|value| !value.is_empty())
            .or(Some("wordpress"))
            .map(str::to_owned);
    }
    if !installers {
        return None;
    }
    let fallback = default_path(kind)?;
    let (vendor, name) = package.name.split_once('/')?;
    let selectors = [
        package.name.clone(),
        format!("type:{kind}"),
        format!("vendor:{vendor}"),
    ];
    let overrides = project.data["extra"]["installer-paths"].as_object();
    let path = overrides
        .and_then(|rules| {
            rules.iter().find_map(|(path, entries)| {
                strings(entries)
                    .iter()
                    .any(|entry| selectors.iter().any(|s| s == *entry))
                    .then_some(path.as_str())
            })
        })
        .unwrap_or(fallback);
    let name = package.field("extra")["installer-name"]
        .as_str()
        .unwrap_or(name);
    Some(
        path.replace("{$name}", name)
            .replace("{$vendor}", vendor)
            .replace("{$type}", kind),
    )
}

fn default_path(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "drupal-core" => "core",
        "drupal-module" => "modules/{$name}",
        "drupal-theme" => "themes/{$name}",
        "drupal-library" => "libraries/{$name}",
        "drupal-profile" => "profiles/{$name}",
        "drupal-database-driver" => "drivers/lib/Drupal/Driver/Database/{$name}",
        "drupal-drush" => "drush/{$name}",
        "drupal-custom-theme" => "themes/custom/{$name}",
        "drupal-custom-module" => "modules/custom/{$name}",
        "drupal-custom-profile" => "profiles/custom/{$name}",
        "drupal-multisite" => "sites/{$name}",
        "drupal-console" => "console/{$name}",
        "drupal-console-language" => "console/language/{$name}",
        "drupal-config" => "config/sync",
        "drupal-recipe" => "recipes/{$name}",
        "wordpress-plugin" => "wp-content/plugins/{$name}",
        "wordpress-theme" => "wp-content/themes/{$name}",
        "wordpress-muplugin" => "wp-content/mu-plugins/{$name}",
        "wordpress-dropin" => "wp-content/{$name}",
        _ => return None,
    })
}
