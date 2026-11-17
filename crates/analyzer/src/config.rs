//! Checker settings: `[check] level = 1` and `paths = ["src"]` in `phera.toml`.
//! Only the documented check table is interpreted; other tables are left alone.

use crate::diagnostic::Format;

#[derive(Clone, Default)]
pub struct Options {
    pub level: u8,
    pub paths: Vec<String>,
    pub exclude: Vec<String>,
    pub baseline: Option<String>,
    pub generate_baseline: Option<String>,
    pub format: Format,
    pub no_progress: bool,
}

impl Options {
    pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            level: 1,
            ..Self::default()
        };
        match std::fs::read_to_string("phera.toml") {
            Ok(source) => options.read_config(&source)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("phera.toml: {e}")),
        }
        let mut arguments = arguments.peekable();
        let mut paths = Vec::new();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--" => {
                    paths.extend(arguments);
                    break;
                }
                "--level" => {
                    options.level = level(&arguments.next().ok_or("--level needs 0 or 1")?)?
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
        Ok(options)
    }

    fn read_config(&mut self, source: &str) -> Result<(), String> {
        let tokens = tokenize(source)?;
        let mut inside = false;
        let mut cursor = 0;
        while cursor < tokens.len() {
            if tokens[cursor] == "[" {
                cursor += 1;
                inside = tokens.get(cursor).is_some_and(|s| s == "check");
                while cursor < tokens.len() && tokens[cursor] != "]" {
                    cursor += 1;
                }
                cursor += 1;
                continue;
            }
            let key = tokens[cursor].clone();
            cursor += 1;
            if tokens.get(cursor).is_none_or(|s| s != "=") {
                if inside {
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
            if !inside {
                continue;
            }
            let scalar = || {
                values
                    .first()
                    .ok_or_else(|| format!("phera.toml: missing {key}"))
            };
            match key.as_str() {
                "level" if !array => self.level = level(scalar()?)?,
                "paths" if array => {
                    self.paths = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
                }
                "exclude" if array => {
                    self.exclude = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
                }
                "baseline" if !array => self.baseline = Some(string(scalar()?)?),
                _ => {
                    return Err(format!(
                        "phera.toml: unsupported check setting or value: {key}"
                    ));
                }
            }
        }
        Ok(())
    }
}

fn level(value: &str) -> Result<u8, String> {
    match value {
        "0" => Ok(0),
        "1" => Ok(1),
        _ => Err("Supported check levels are 0 and 1".into()),
    }
}

fn string(value: &str) -> Result<String, String> {
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
