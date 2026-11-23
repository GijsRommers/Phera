//! A real framework application for a test, built around the `vendor/`
//! directory of an application that is installed on this machine.
//!
//! ```text
//! PHERA_LARAVEL_APP=/path/to/an/installed/laravel/app cargo test -p phera-cli --test warmup_laravel
//! ```
//!
//! The repository holds no framework: a test names the installed application
//! through an environment variable and is skipped, visibly, without it. The
//! test's own application (its routes, models and configuration) is a
//! directory of this repository; it is copied to a temporary directory and
//! gets the installed application's packages:
//!
//! ```text
//! <temporary>/app, routes, ...     copied from the test's skeleton
//! <temporary>/vendor/composer/     copied: Composer's autoloader derives the
//!                                  project directory from its own location
//! <temporary>/vendor/<package>     links to the installed packages
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use phera_extension_api::testing::serve::Server;

/// The installed application `variable` names, or `None` after saying on
/// standard error that the test is skipped.
pub fn installed_application(variable: &str) -> Option<PathBuf> {
    let application = std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.join("vendor/autoload.php").is_file());
    if application.is_none() {
        eprintln!("skipped: set {variable} to an installed application (a directory with vendor/)");
    }
    application
}

/// A copy of a skeleton application with the packages of an installed one,
/// removed when the test ends.
pub struct Project {
    pub root: PathBuf,
}

impl Project {
    pub fn new(skeleton: &Path, installed: &Path) -> Project {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "phera-framework-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        copy_directory(skeleton, &root);
        let root = root.canonicalize().expect("canonical project directory");
        link_packages(&installed.join("vendor"), &root.join("vendor"));
        Project { root }
    }

    pub fn write(&self, file: &str, content: &str) {
        std::fs::write(self.root.join(file), content).expect("write a project file");
    }

    pub fn document_root(&self) -> PathBuf {
        self.root.join("public")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // Links are removed, not followed: the installed packages stay.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create a directory");
    for entry in std::fs::read_dir(from).expect("read a directory") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

fn link_packages(installed: &Path, vendor: &Path) {
    std::fs::create_dir_all(vendor).expect("create vendor");
    for entry in std::fs::read_dir(installed).expect("read the installed vendor directory") {
        let entry = entry.expect("vendor entry");
        let target = vendor.join(entry.file_name());
        if entry.file_name() == "composer" {
            copy_directory(&entry.path(), &target);
        } else if entry.file_type().expect("file type").is_dir() {
            std::os::unix::fs::symlink(entry.path(), &target).expect("link a package");
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a vendor file");
        }
    }
}

/// What an application answered, without what the two HTTP servers add
/// themselves, and with the headers sorted by name.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Answer {
    pub fn parse(raw: &str) -> Answer {
        let (head, body) = raw.split_once("\r\n\r\n").expect("an HTTP response");
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse().ok())
            .expect("a status line");
        let mut headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
            .filter(|(name, _)| {
                !matches!(
                    name.as_str(),
                    "host"
                        | "date"
                        | "connection"
                        | "content-length"
                        | "x-powered-by"
                        | "transfer-encoding"
                )
            })
            .collect();
        // Stable, so several Set-Cookie headers keep their order.
        headers.sort_by(|left, right| left.0.cmp(&right.0));
        Answer {
            status,
            headers,
            body: body.to_string(),
        }
    }

    /// The values of every header called `name` (lower case).
    pub fn header(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// The `name=value` pairs of the cookies this answer sets, to send back.
    pub fn cookies(&self) -> String {
        self.header("set-cookie")
            .iter()
            .filter_map(|cookie| cookie.split(';').next())
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// This answer with the value and the expiry of every cookie it sets
    /// removed: a session cookie differs between any two requests.
    pub fn without_cookie_values(mut self) -> Answer {
        for (name, value) in &mut self.headers {
            if name == "set-cookie" {
                *value = value
                    .split(';')
                    .enumerate()
                    .map(|(index, part)| match (index, part.split_once('=')) {
                        (0, Some((cookie, _))) => format!("{cookie}=…"),
                        (_, Some((attribute, _)))
                            if attribute.trim().eq_ignore_ascii_case("expires") =>
                        {
                            " expires=…".to_string()
                        }
                        _ => part.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(";");
            }
        }
        self
    }

    /// This answer with every occurrence of `from` replaced by `to`, in the
    /// headers and the body: for a server's own address.
    pub fn replacing(mut self, from: &str, to: &str) -> Answer {
        self.body = self.body.replace(from, to);
        for (_, value) in &mut self.headers {
            *value = value.replace(from, to);
        }
        self
    }
}

/// One request, answered: a worker that is still booting does not accept
/// yet, so the first request of a test waits for it.
pub fn get(server: &Server, target: &str, headers: &[(&str, &str)]) -> Answer {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match server.try_request_once("GET", target, headers, b"") {
            Ok(raw) if raw.contains("\r\n\r\n") => {
                return Answer::parse(&raw).replacing(&server.address, "SERVER");
            }
            _ if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
            other => panic!("{target}: no answer from {}: {other:?}", server.address),
        }
    }
}
