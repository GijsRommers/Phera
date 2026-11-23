//! Quote script arguments before handing a root command to the system shell.
use anyhow::{Context, Result, bail};
use std::process::Command;

pub(super) fn forward(command: &str, args: &[String]) -> String {
    if command.contains("@no_additional_args") {
        return command.replace("@no_additional_args", "");
    }
    let args = shell_arguments(args);
    if command.contains("@additional_args") {
        command.replace("@additional_args", &args)
    } else if args.is_empty() {
        command.to_owned()
    } else {
        format!("{command} {args}")
    }
}

pub(super) fn shell_arguments(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn split_words(input: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut input = input.chars();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    while let Some(character) = input.next() {
        match (quote, character) {
            (Some(open), close) if open == close => quote = None,
            (None, '\'' | '"') => {
                started = true;
                quote = Some(character);
            }
            (None | Some('"'), '\\') => {
                started = true;
                word.push(
                    input
                        .next()
                        .context("Missing character after a script escape.")?,
                );
            }
            (None, space) if space.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                started = true;
                word.push(character);
            }
        }
    }
    if quote.is_some() {
        bail!("Unclosed quote in Composer script arguments.");
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

pub(super) fn shell(command: String) -> Command {
    let mut shell = if cfg!(windows) {
        Command::new("cmd")
    } else {
        Command::new("sh")
    };
    shell
        .arg(if cfg!(windows) { "/C" } else { "-c" })
        .arg(command);
    shell
}

pub(super) fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
