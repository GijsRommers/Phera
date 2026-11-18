//! The plain project: a small application without packages.
//!
//! Its files are the ones under `crates/cli/templates/new/`, built into the
//! binary. They are ordinary PHP, kept there as files so they can be read,
//! checked and tested like any other project.

use std::fs;
use std::io;
use std::path::Path;

/// One file of a new project.
struct TemplateFile {
    /// Where it goes in the project.
    path: &'static str,
    /// Its text, with `{{name}}` and `{{package}}` still to fill in.
    contents: &'static str,
}

/// Two files have another name in the template directory, where a real
/// `.gitignore` or `.env` would act on this repository.
const FILES: &[TemplateFile] = &[
    TemplateFile {
        path: ".env",
        contents: include_str!("../../templates/new/env"),
    },
    TemplateFile {
        path: ".gitignore",
        contents: include_str!("../../templates/new/gitignore"),
    },
    TemplateFile {
        path: "README.md",
        contents: include_str!("../../templates/new/README.md"),
    },
    TemplateFile {
        path: "composer.json",
        contents: include_str!("../../templates/new/composer.json"),
    },
    TemplateFile {
        path: "public/index.php",
        contents: include_str!("../../templates/new/public/index.php"),
    },
    TemplateFile {
        path: "src/Application.php",
        contents: include_str!("../../templates/new/src/Application.php"),
    },
    TemplateFile {
        path: "src/Http/Request.php",
        contents: include_str!("../../templates/new/src/Http/Request.php"),
    },
    TemplateFile {
        path: "src/Http/Response.php",
        contents: include_str!("../../templates/new/src/Http/Response.php"),
    },
];

/// Writes the files into `directory`, with the project's name filled in.
pub(super) fn write_files(directory: &Path, name: &str) -> Result<(), String> {
    let package = package_name(name);
    for file in FILES {
        let text = file
            .contents
            .replace("{{name}}", name)
            .replace("{{package}}", &package);
        write(&directory.join(file.path), &text)
            .map_err(|error| format!("cannot write {}: {error}", file.path))?;
    }
    Ok(())
}

/// Composer wants lower case, with single separators between the words:
/// `My_Shop..2` becomes `my-shop-2`.
fn package_name(project: &str) -> String {
    project
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

fn write(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_name_is_lower_case_with_single_separators() {
        assert_eq!(package_name("shop"), "shop");
        assert_eq!(package_name("My_Shop..2"), "my-shop-2");
        assert_eq!(package_name("api-"), "api");
    }

    #[test]
    fn every_placeholder_in_the_templates_is_one_we_fill_in() {
        for file in FILES {
            let filled = file
                .contents
                .replace("{{name}}", "")
                .replace("{{package}}", "");
            assert!(!filled.contains("{{"), "{}", file.path);
        }
    }
}
