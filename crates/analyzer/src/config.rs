//! Checker settings: `[check] level = 1` and `paths = ["src"]` in `phera.toml`.
//! Only the documented check table is interpreted; other tables are left alone.
//! For example, `new Missing` is checked at the configured level for these paths.

use crate::diagnostic::Format;

#[derive(Clone)]
pub struct Options {
    pub level: u8,
    pub paths: Vec<String>,
    pub scan: Vec<String>,
    pub exclude: Vec<String>,
    pub baseline: Option<String>,
    pub generate_baseline: Option<String>,
    pub format: Format,
    pub no_progress: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            level: 1,
            paths: Vec::new(),
            exclude: Vec::new(),
            scan: Vec::new(),
            baseline: None,
            generate_baseline: None,
            format: Format::Text,
            no_progress: false,
        }
    }
}

impl Options {
    pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        match std::fs::read_to_string("phera.toml") {
            Ok(source) => options.read_config(&source)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("phera.toml: {e}")),
        }
        let mut arguments = arguments.peekable();
        let mut paths = Vec::new();
        let mut use_phpstan_baseline = false;
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--" => {
                    paths.extend(arguments);
                    break;
                }
                "--level" => {
                    options.level = level(&arguments.next().ok_or("--level needs a value")?)?
                }
                "--format" => {
                    options.format = match arguments.next().as_deref() {
                        Some("text") => Format::Text,
                        Some("json") => Format::Json,
                        Some("github") => Format::Github,
                        _ => return Err("--format needs text, json or github".into()),
                    }
                }
                "--no-progress" => options.no_progress = true,
                "--baseline" => use_phpstan_baseline = true,
                "--generate-baseline" => {
                    options.generate_baseline =
                        Some(arguments.next().ok_or("--generate-baseline needs a file")?)
                }
                value if value.starts_with('-') => {
                    return Err(format!("Unknown check option: {value}"));
                }
                _ => paths.push(argument),
            }
        }
        if !paths.is_empty() {
            options.paths = paths;
        }
        if options.paths.is_empty() {
            return Err("check needs paths or [check].paths in phera.toml".into());
        }
        if use_phpstan_baseline && options.baseline.is_none() {
            options.baseline = Some(default_baseline(&options.paths));
        }
        Ok(options)
    }

    fn read_config(&mut self, source: &str) -> Result<(), String> {
        read_tables(
            source,
            |table| table == "check",
            |_, key, values, array| self.setting(key, values, array),
        )
    }

    fn setting(&mut self, key: &str, values: Vec<String>, array: bool) -> Result<(), String> {
        let scalar = || {
            values
                .first()
                .ok_or_else(|| format!("phera.toml: missing {key}"))
        };
        match key {
            "level" if !array => self.level = level(scalar()?)?,
            "paths" if array => {
                self.paths = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "scan" if array => {
                self.scan = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "exclude" if array => {
                self.exclude = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "baseline" if !array => self.baseline = Some(string(scalar()?)?),
            "format" if !array => {
                self.format = match string(scalar()?)?.as_str() {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    "github" => Format::Github,
                    _ => return Err("phera.toml: format must be text, json or github".into()),
                }
            }
            "no_progress" | "no-progress" if !array => {
                self.no_progress = match scalar()?.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err("phera.toml: no_progress must be true or false".into()),
                }
            }
            _ => {
                return Err(format!(
                    "phera.toml: unsupported check setting or value: {key}"
                ));
            }
        }
        Ok(())
    }
}

fn default_baseline(paths: &[String]) -> String {
    const FILE: &str = "phpstan-baseline.neon";
    if std::path::Path::new(FILE).is_file() {
        return FILE.into();
    }
    paths
        .iter()
        .filter_map(|path| std::path::Path::new(path).canonicalize().ok())
        .find_map(|path| {
            path.ancestors()
                .map(|directory| directory.join(FILE))
                .find(|file| file.is_file())
        })
        .map_or_else(|| FILE.into(), |file| file.to_string_lossy().into_owned())
}

/// Read the requested tables from settings shared by the analyzer commands.
pub(crate) fn read_tables(
    source: &str,
    interested: impl Fn(&str) -> bool,
    mut setting: impl FnMut(&str, &str, Vec<String>, bool) -> Result<(), String>,
) -> Result<(), String> {
    let tokens = tokenize(source)?;
    let mut table = String::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if tokens[cursor] == "[" {
            cursor += 1;
            table = tokens.get(cursor).cloned().unwrap_or_default();
            while cursor < tokens.len() && tokens[cursor] != "]" {
                cursor += 1;
            }
            cursor += 1;
            continue;
        }
        let key = tokens[cursor].clone();
        cursor += 1;
        if tokens.get(cursor).is_none_or(|s| s != "=") {
            if interested(&table) {
                return Err(format!("phera.toml: expected '=' after {key}"));
            }
            continue;
        }
        cursor += 1;
        let mut values = Vec::new();
        let array = tokens.get(cursor).is_some_and(|s| s == "[");
        if array {
            cursor += 1;
            while cursor < tokens.len() && tokens[cursor] != "]" {
                if tokens[cursor] != "," {
                    values.push(tokens[cursor].clone());
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
        if !interested(&table) {
            continue;
        }
        setting(&table, &key, values, array)?;
    }
    Ok(())
}

fn level(value: &str) -> Result<u8, String> {
    match value {
        "0" => Ok(0),
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        "4" => Ok(4),
        "5" => Ok(5),
        "6" => Ok(6),
        "7" => Ok(7),
        "8" => Ok(8),
        "9" => Ok(9),
        "10" | "max" => Ok(10),
        _ => Err("Supported check levels are 0 through 10, or max".into()),
    }
}

pub(crate) fn string(value: &str) -> Result<String, String> {
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
