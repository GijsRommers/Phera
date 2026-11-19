//! Read the project configuration shared by runtime startup and the checker.
//!
//! With `[runtime]` and `php = "8.6"`, `echo PHP_MINOR_VERSION;` prints 6.

use phera_source::LanguageLevel;
use std::path::{Path, PathBuf};

pub struct ProjectConfig {
    path: PathBuf,
    settings: Vec<Setting>,
}

pub(super) struct Setting {
    section: String,
    pub(super) key: String,
    pub(super) values: Vec<String>,
    pub(super) array: bool,
}

impl ProjectConfig {
    /// Finds the nearest configuration file, starting at the project directory.
    pub fn find(start: &Path) -> Result<Option<Self>, String> {
        let start = start
            .canonicalize()
            .map_err(|error| format!("{}: {error}", start.display()))?;
        for directory in start.ancestors() {
            let path = directory.join("phera.toml");
            match std::fs::read_to_string(&path) {
                Ok(source) => {
                    let settings = read_settings(&source)?;
                    return Ok(Some(Self { path, settings }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("{}: {error}", path.display())),
            }
        }
        Ok(None)
    }

    pub(super) fn settings<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a Setting> {
        self.settings
            .iter()
            .filter(move |setting| setting.section == section)
    }

    pub fn php(&self) -> Result<Option<LanguageLevel>, String> {
        let Some(setting) = self
            .settings("runtime")
            .find(|setting| setting.key == "php")
        else {
            return Ok(None);
        };
        let [value] = setting.values.as_slice() else {
            return Err(self.php_setting_error());
        };
        if setting.array {
            return Err(self.php_setting_error());
        }
        string(value)
            .map_err(|_| self.php_setting_error())?
            .parse()
            .map(Some)
            .map_err(|error| format!("{}: {error}", self.path.display()))
    }

    fn php_setting_error(&self) -> String {
        format!(
            "{}: [runtime].php must be a quoted version, 8.5 or 8.6.",
            self.path.display()
        )
    }
}

fn read_settings(source: &str) -> Result<Vec<Setting>, String> {
    let tokens = tokenize(source)?;
    let mut section = String::new();
    let mut settings = Vec::new();
    let mut cursor = 0;
    while let Some(token) = tokens.get(cursor) {
        cursor += 1;
        if token == "[" {
            let start = cursor;
            while cursor < tokens.len() && tokens[cursor] != "]" {
                cursor += 1;
            }
            section = tokens[start..cursor].join("");
            cursor += 1;
            continue;
        }
        if tokens.get(cursor).is_none_or(|token| token != "=") {
            if matches!(section.as_str(), "check" | "runtime") {
                return Err(format!("phera.toml: expected '=' after {token}"));
            }
            continue;
        }
        cursor += 1;
        let mut values = Vec::new();
        let array = tokens.get(cursor).is_some_and(|token| token == "[");
        if array {
            cursor += 1;
            while let Some(value) = tokens.get(cursor).filter(|value| *value != "]") {
                if value != "," {
                    values.push(value.clone());
                }
                cursor += 1;
            }
            if cursor == tokens.len() {
                return Err("phera.toml: unclosed array".into());
            }
            cursor += 1;
        } else if let Some(value) = tokens.get(cursor) {
            values.push(value.clone());
            cursor += 1;
        }
        settings.push(Setting {
            section: section.clone(),
            key: token.clone(),
            values,
            array,
        });
    }
    Ok(settings)
}

pub(super) fn string(value: &str) -> Result<String, String> {
    value
        .strip_prefix('"')
        .map(String::from)
        .ok_or("phera.toml: expected a string".into())
}

fn tokenize(source: &str) -> Result<Vec<String>, String> {
    let mut input = source.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(c) = input.next() {
        match c {
            c if c.is_whitespace() => {}
            '#' => {
                while input.peek().is_some_and(|c| *c != '\n') {
                    input.next();
                }
            }
            '[' | ']' | '=' | ',' => tokens.push(c.to_string()),
            '"' | '\'' => {
                let mut value = String::from("\"");
                let mut closed = false;
                while let Some(next) = input.next() {
                    if next == c {
                        closed = true;
                        break;
                    }
                    if next == '\\' && c == '"' {
                        value.push(match input.next().ok_or("phera.toml: unfinished escape")? {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            '\\' => '\\',
                            '"' => '"',
                            _ => return Err("phera.toml: unsupported string escape".into()),
                        });
                    } else {
                        value.push(next);
                    }
                }
                if !closed {
                    return Err("phera.toml: unclosed string".into());
                }
                tokens.push(value);
            }
            _ => {
                let mut value = c.to_string();
                while input.peek().is_some_and(|c| {
                    !c.is_whitespace() && !matches!(c, '#' | '[' | ']' | '=' | ',')
                }) {
                    if let Some(c) = input.next() {
                        value.push(c);
                    }
                }
                tokens.push(value);
            }
        }
    }
    Ok(tokens)
}
