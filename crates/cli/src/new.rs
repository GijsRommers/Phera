//! `phera new`: start a project. It asks for a name and a kind of project
//! (or takes them from the command line), then writes a small application or
//! fetches a framework's official starting project.

mod composer;
mod plain;

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub(crate) struct NewOptions {
    /// Where the project goes; its last part is the name of the project.
    /// `None`: ask.
    pub directory: Option<PathBuf>,
    /// `--template`. `None`: ask.
    pub template: Option<Template>,
    /// `--no-install` leaves out fetching packages.
    pub install: bool,
}

/// The kinds of project `phera new` can start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Template {
    Plain,
    Symfony,
    SymfonyWebApp,
    Laravel,
}

impl Template {
    /// In the order the question lists them; the first is the default.
    const ALL: [Template; 4] = [
        Template::Plain,
        Template::Symfony,
        Template::SymfonyWebApp,
        Template::Laravel,
    ];

    /// The word for `--template`.
    fn key(self) -> &'static str {
        match self {
            Template::Plain => "plain",
            Template::Symfony => "symfony",
            Template::SymfonyWebApp => "symfony-webapp",
            Template::Laravel => "laravel",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Template::Plain => "a small JSON application without packages",
            Template::Symfony => "Symfony's official skeleton: for an API or a service",
            Template::SymfonyWebApp => "the skeleton plus Twig, Doctrine, forms and security",
            Template::Laravel => "the official Laravel application",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Template> {
        Template::ALL
            .into_iter()
            .find(|template| template.key() == key)
    }

    /// The framework this kind of project is built on, if any.
    fn framework(self) -> Option<&'static str> {
        match self {
            Template::Plain => None,
            Template::Symfony | Template::SymfonyWebApp => Some("Symfony"),
            Template::Laravel => Some("Laravel"),
        }
    }

    /// Phera does not run the frameworks yet: they need language features
    /// that are still being built. Change this when that changes; the
    /// message below then disappears.
    fn runs_on_phera(self) -> bool {
        self.framework().is_none()
    }
}

/// The `--template` words, for messages.
pub(crate) fn template_keys() -> String {
    Template::ALL.map(Template::key).join(", ")
}

pub(crate) fn project(options: &NewOptions) -> ExitCode {
    match create(options, &mut io::stdin().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("phera new: {message}");
            ExitCode::from(1)
        }
    }
}

/// `answers` is where the replies to questions come from: the terminal.
fn create(options: &NewOptions, answers: &mut impl BufRead) -> Result<(), String> {
    let directory = match &options.directory {
        Some(directory) => directory.clone(),
        None => PathBuf::from(ask(answers, "Project name: ")?),
    };
    let name = project_name(&directory)?;
    refuse_existing_files(&directory)?;
    let template = match options.template {
        Some(template) => template,
        None => ask_template(answers)?,
    };

    let steps = composer::steps(template, &directory, options.install);
    let mut needs_composer_install = !options.install;
    match template.framework() {
        None => {
            plain::write_files(&directory, &name)?;
            // The files are there whatever Composer does next.
            if let Err(failure) = composer::run(&steps, &directory) {
                needs_composer_install = true;
                println!("{}", explain(&failure));
            }
        }
        Some(framework) => composer::run(&steps, &directory).map_err(|failure| {
            format!(
                "a {framework} project is fetched with Composer. {}",
                explain(&failure)
            )
        })?,
    }

    println!("\nCreated {name}.\n\nNext:\n  cd {}", directory.display());
    if needs_composer_install {
        println!("  composer install        fetches packages and writes the class loader");
    }
    match template.framework() {
        Some(framework) if !template.runs_on_phera() => println!(
            "  php -S 127.0.0.1:8080 -t public\n\n\
             Phera does not run {framework} applications yet. When it does, the last step is:\n  \
             phera serve public"
        ),
        _ => println!("  phera serve public      then open http://127.0.0.1:8080/"),
    }
    Ok(())
}

fn explain(failure: &composer::Failure) -> String {
    match failure {
        composer::Failure::Missing => {
            "Composer is not installed on this machine: https://getcomposer.org/download/"
                .to_string()
        }
        composer::Failure::Failed(reason) => format!("That did not work: {reason}."),
    }
}

/// Prints the question and returns the reply without the line end. An empty
/// reply is returned as it is; the end of the input is an error, so a script
/// that pipes nothing in gets a message, not a hang.
fn ask(answers: &mut impl BufRead, question: &str) -> Result<String, String> {
    print!("{question}");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut reply = String::new();
    match answers.read_line(&mut reply) {
        Ok(0) => Err(format!(
            "no answer to \"{}\". Give it on the command line: phera new <directory> --template <{}>",
            question.trim_end_matches([':', ' ']),
            template_keys().replace(", ", "|")
        )),
        Ok(_) => Ok(reply.trim().to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Asks until the reply is a number from the list, one of the names, or
/// empty for the first.
fn ask_template(answers: &mut impl BufRead) -> Result<Template, String> {
    println!("\nWhat kind of project?");
    for (index, template) in Template::ALL.into_iter().enumerate() {
        println!(
            "  {}. {:<16}{}",
            index + 1,
            template.key(),
            template.description()
        );
    }
    loop {
        let reply = ask(answers, "Number or name [1]: ")?;
        let chosen = match reply.parse::<usize>() {
            Ok(number) => number
                .checked_sub(1)
                .and_then(|index| Template::ALL.get(index).copied()),
            Err(_) if reply.is_empty() => Some(Template::ALL[0]),
            Err(_) => Template::from_key(&reply),
        };
        match chosen {
            Some(template) => return Ok(template),
            None => println!("{reply:?} is not in the list."),
        }
    }
}

/// The last part of the path names the project. It ends up inside PHP,
/// JSON and Markdown text, so it may only hold characters that mean nothing
/// special in any of them.
fn project_name(directory: &Path) -> Result<String, String> {
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("give the directory of the new project, for example: phera new shop")?;
    let starts_plainly = name.starts_with(|first: char| first.is_ascii_alphanumeric());
    let only_plain = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if starts_plainly && only_plain {
        Ok(name.to_string())
    } else {
        Err(format!(
            "the project name {name:?} must start with a letter or digit and hold only \
             letters, digits, '-', '_' and '.'"
        ))
    }
}

/// A new project never overwrites anything: the directory must be missing
/// or empty.
fn refuse_existing_files(directory: &Path) -> Result<(), String> {
    match fs::read_dir(directory) {
        Ok(mut entries) => match entries.next() {
            None => Ok(()),
            Some(_) => Err(format!(
                "{} already exists and is not empty",
                directory.display()
            )),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot use {}: {error}", directory.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_part_of_the_path_names_the_project() {
        assert_eq!(project_name(Path::new("shop")), Ok("shop".to_string()));
        assert_eq!(
            project_name(Path::new("sites/My_Shop.2")),
            Ok("My_Shop.2".to_string())
        );
    }

    #[test]
    fn names_that_would_need_escaping_are_refused() {
        for name in [
            "my shop", "it's", "a\"b", "-shop", ".hidden", "café", "..", "",
        ] {
            assert!(project_name(Path::new(name)).is_err(), "{name}");
        }
    }

    #[test]
    fn the_kind_of_project_is_a_number_a_name_or_the_default() {
        let chosen = |typed: &str| ask_template(&mut typed.as_bytes());
        assert_eq!(chosen("\n"), Ok(Template::Plain));
        assert_eq!(chosen("2\n"), Ok(Template::Symfony));
        assert_eq!(chosen("laravel\n"), Ok(Template::Laravel));
        assert_eq!(chosen(" symfony-webapp \n"), Ok(Template::SymfonyWebApp));
        // A reply that is not in the list is asked again.
        assert_eq!(chosen("0\n9\nrails\n4\n"), Ok(Template::Laravel));
    }

    #[test]
    fn a_question_nobody_answers_is_an_error_that_names_the_options() {
        let error = ask_template(&mut "".as_bytes()).expect_err("no answer");
        assert!(
            error.contains("--template <plain|symfony|symfony-webapp|laravel>"),
            "{error}"
        );
    }

    #[test]
    fn every_kind_has_its_own_word() {
        for template in Template::ALL {
            assert_eq!(Template::from_key(template.key()), Some(template));
        }
        assert_eq!(Template::from_key("rails"), None);
    }
}
