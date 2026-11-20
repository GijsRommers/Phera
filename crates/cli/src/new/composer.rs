//! Running Composer: it fetches a framework's official starting project and
//! installs packages.
//!
//! This is the fallback for templates the native installer cannot create.

use std::io;
use std::path::Path;
use std::process::Command;

use super::Template;
use super::native::SYMFONY_VERSION;

/// One run of Composer.
#[derive(Debug, PartialEq)]
pub(super) struct Step {
    /// What follows `composer` on the command line.
    pub arguments: Vec<String>,
    /// True when the step runs inside the new project, false when it runs
    /// where the user is and creates the project.
    pub inside_project: bool,
}

/// The Composer runs that make a project of this kind in `directory`.
///
/// Without `install` nothing is downloaded but the starting project itself:
/// its packages wait for a later `composer install`.
pub(super) fn steps(template: Template, directory: &Path, install: bool) -> Vec<Step> {
    let create_project = |package: &str| {
        let mut arguments = vec![
            "create-project".to_string(),
            package.to_string(),
            directory.display().to_string(),
            "--no-interaction".to_string(),
        ];
        if !install {
            // The project's own setup scripts need its packages.
            arguments.extend(["--no-install".to_string(), "--no-scripts".to_string()]);
        }
        Step {
            arguments,
            inside_project: false,
        }
    };
    let in_project = |arguments: &[&str]| {
        let mut arguments: Vec<String> = arguments.iter().map(|part| part.to_string()).collect();
        if !install {
            arguments.push("--no-install".to_string());
        }
        Step {
            arguments,
            inside_project: true,
        }
    };
    let symfony = format!("symfony/skeleton:{SYMFONY_VERSION}");

    match template {
        // Nothing to fetch, but the class loader is Composer's to write.
        Template::Plain if !install => Vec::new(),
        Template::Plain => vec![in_project(&["install", "--no-interaction"])],
        Template::Symfony => vec![create_project(&symfony)],
        Template::SymfonyWebApp => vec![
            create_project(&symfony),
            in_project(&["require", "webapp", "--no-interaction"]),
        ],
        Template::Laravel => vec![create_project("laravel/laravel")],
    }
}

/// Why a Composer run did not do its work.
pub(super) enum Failure {
    /// Composer is not on this machine.
    Missing,
    /// It ran and reported a problem itself, or could not be started.
    Failed(String),
}

/// Runs the steps in order, with Composer's own output on the terminal.
pub(super) fn run(steps: &[Step], directory: &Path) -> Result<(), Failure> {
    for step in steps {
        println!("\n> composer {}", step.arguments.join(" "));
        let mut command = Command::new("composer");
        command.args(&step.arguments);
        if step.inside_project {
            command.current_dir(directory);
        }
        match command.status() {
            Ok(status) if status.success() => {}
            Ok(status) => return Err(Failure::Failed(format!("composer stopped ({status})"))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(Failure::Missing),
            Err(error) => return Err(Failure::Failed(format!("cannot start composer: {error}"))),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_lines(template: Template, install: bool) -> Vec<String> {
        steps(template, Path::new("shop"), install)
            .into_iter()
            .map(|step| step.arguments.join(" "))
            .collect()
    }

    #[test]
    fn frameworks_start_from_their_official_project() {
        assert_eq!(
            command_lines(Template::Symfony, true),
            ["create-project symfony/skeleton:7.4.* shop --no-interaction"]
        );
        assert_eq!(
            command_lines(Template::SymfonyWebApp, true),
            [
                "create-project symfony/skeleton:7.4.* shop --no-interaction",
                "require webapp --no-interaction"
            ]
        );
        assert_eq!(
            command_lines(Template::Laravel, true),
            ["create-project laravel/laravel shop --no-interaction"]
        );
    }

    #[test]
    fn without_install_no_packages_are_fetched() {
        assert_eq!(
            command_lines(Template::Laravel, false),
            ["create-project laravel/laravel shop --no-interaction --no-install --no-scripts"]
        );
        assert_eq!(
            command_lines(Template::SymfonyWebApp, false)[1],
            "require webapp --no-interaction --no-install"
        );
    }

    #[test]
    fn a_plain_project_only_needs_the_class_loader() {
        assert_eq!(
            command_lines(Template::Plain, true),
            ["install --no-interaction"]
        );
        assert!(command_lines(Template::Plain, false).is_empty());
    }

    #[test]
    fn only_the_first_step_of_a_framework_runs_outside_the_project() {
        let steps = steps(Template::SymfonyWebApp, Path::new("shop"), true);
        assert_eq!(
            steps
                .iter()
                .map(|step| step.inside_project)
                .collect::<Vec<_>>(),
            [false, true]
        );
    }
}
