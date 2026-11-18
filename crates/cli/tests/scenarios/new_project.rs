//! `phera new`: the project it writes, and that the project answers requests
//! as its README says.

use std::path::Path;
use std::process::{Command, Output};

use super::super::Served;

/// What `composer install` writes for a project without packages, reduced to
/// the one rule in the project's `composer.json`: `App\Name` is
/// `src/Name.php`. The tests must not need Composer or the network.
const CLASS_LOADER: &str = "<?php spl_autoload_register(function (string $class): void {
    if (str_starts_with($class, 'App\\\\')) {
        require dirname(__DIR__) . '/src/' . str_replace('\\\\', '/', substr($class, 4)) . '.php';
    }
});";

fn phera_new(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg("new")
        .args(arguments)
        .output()
        .expect("run phera new")
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn a_new_project_answers_requests() {
    let root = Served::root_of("new-project").join("My_Shop");
    let created = phera_new(&[
        "--template",
        "plain",
        "--no-install",
        root.to_str().expect("path"),
    ]);
    assert!(created.status.success(), "{created:?}");
    let said = String::from_utf8_lossy(&created.stdout);
    assert!(said.contains("composer install"), "{said}");
    assert!(said.contains("phera serve public"), "{said}");

    // The name is filled in: as it was typed where people read it, and in
    // Composer's form in composer.json.
    assert!(text(&root.join("README.md")).starts_with("# My_Shop\n"));
    assert!(text(&root.join("composer.json")).contains("\"name\": \"app/my-shop\""));
    assert!(text(&root.join(".env")).contains("APP_NAME=\"My_Shop\""));
    assert!(text(&root.join(".gitignore")).contains("/vendor/"));

    let served = Served::start_in(
        "new-project",
        1,
        &[("My_Shop/vendor/autoload.php", CLASS_LOADER)],
        "My_Shop/public",
        &[],
    );
    assert_eq!(
        served.get("/"),
        (
            200,
            format!(
                "{{\"application\":\"My_Shop\",\"environment\":\"dev\",\"php\":\"{}\"}}",
                phera_extension_api::PHP_VERSION
            )
        )
    );
    assert_eq!(
        served.get("/hello?name=Gijs"),
        (200, "{\"hello\":\"Gijs\"}".to_string())
    );
    assert_eq!(
        served.get("/hello"),
        (200, "{\"hello\":\"world\"}".to_string())
    );
    assert_eq!(
        served.get("/missing"),
        (404, "{\"error\":\"Not found\"}".to_string())
    );
}

#[test]
fn a_new_project_never_overwrites_files() {
    let root = Served::root_of("new-project-existing");
    std::fs::create_dir_all(&root).expect("create directory");
    std::fs::write(root.join("notes.txt"), "mine").expect("write file");

    let refused = phera_new(&["--template", "laravel", root.to_str().expect("path")]);
    assert_eq!(refused.status.code(), Some(1));
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(said.contains("already exists and is not empty"), "{said}");
    assert_eq!(text(&root.join("notes.txt")), "mine");
    assert!(!root.join("composer.json").exists());

    std::fs::remove_dir_all(&root).expect("clean up");
}

#[test]
fn mistakes_on_the_command_line_are_refused() {
    for arguments in [
        &["--template", "plain", "my shop"][..],
        &["a", "b"],
        &["--force", "a"],
        &["--template", "rails", "a"],
        &["--template"],
    ] {
        let refused = phera_new(arguments);
        assert!(!refused.status.success(), "{arguments:?}");
        assert!(!Path::new("a").exists() && !Path::new("my shop").exists());
    }
}

/// The questions read their answers from the terminal. With nothing there
/// (a script, a pipeline) the command says what to pass instead of waiting.
#[test]
fn without_a_terminal_the_questions_become_an_error() {
    let unanswered = Command::new(env!("CARGO_BIN_EXE_phera"))
        .arg("new")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run phera new");
    assert_eq!(unanswered.status.code(), Some(1));
    let said = String::from_utf8_lossy(&unanswered.stderr);
    assert!(said.contains("phera new <directory> --template"), "{said}");
}
