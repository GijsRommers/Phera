//! MessageFormat 1 has balanced nested branch messages and apostrophe quoting.

use std::{iter::Peekable, str::Chars};

#[derive(Debug, Clone)]
pub enum Node {
    Text(String),
    Pound,
    Argument(String, Argument),
}

#[derive(Debug, Clone)]
pub enum Argument {
    Simple,
    Number(String),
    Date(String),
    Time(String),
    Select(Choices),
    Plural {
        choices: Choices,
        offset: f64,
        ordinal: bool,
    },
}

type Choices = Vec<(String, Vec<Node>)>;
type Input<'a> = Peekable<Chars<'a>>;

pub fn parse(pattern: &str) -> Option<Vec<Node>> {
    message(&mut pattern.chars().peekable(), false, false, 0)
}

fn message(input: &mut Input<'_>, nested: bool, plural: bool, depth: usize) -> Option<Vec<Node>> {
    if depth > 64 {
        return None;
    }
    let mut nodes = Vec::new();
    let mut literal = String::new();
    while let Some(character) = input.next() {
        let token = match character {
            '}' if nested => {
                push_text(&mut nodes, &mut literal);
                return Some(nodes);
            }
            '{' => argument(input, depth + 1)?,
            '#' if plural => Node::Pound,
            '\'' => {
                quote(input, &mut literal, plural);
                continue;
            }
            _ => {
                literal.push(character);
                continue;
            }
        };
        push_text(&mut nodes, &mut literal);
        nodes.push(token);
    }
    push_text(&mut nodes, &mut literal);
    (!nested).then_some(nodes)
}

fn push_text(nodes: &mut Vec<Node>, literal: &mut String) {
    if !literal.is_empty() {
        nodes.push(Node::Text(std::mem::take(literal)));
    }
}

fn quote(input: &mut Input<'_>, output: &mut String, plural: bool) {
    match input.peek() {
        Some('\'') => {
            input.next();
            output.push('\'');
            return;
        }
        Some('{' | '}') => {}
        Some('#') if plural => {}
        _ => {
            output.push('\'');
            return;
        }
    }
    while let Some(character) = input.next() {
        if character == '\'' && input.next_if_eq(&'\'').is_none() {
            return;
        }
        output.push(character);
    }
}

fn argument(input: &mut Input<'_>, depth: usize) -> Option<Node> {
    let key = take_until(input, &[',', '}']);
    if key.is_empty() || key.chars().any(|ch| ch.is_whitespace() || ch == '{') {
        return None;
    }
    if input.next_if_eq(&'}').is_some() {
        return Some(Node::Argument(key, Argument::Simple));
    }
    input.next_if_eq(&',')?;
    let kind = take_until(input, &[',', '}']).to_ascii_lowercase();
    let value = match kind.as_str() {
        "select" | "plural" | "selectordinal" => {
            input.next_if_eq(&',')?;
            let plural = kind != "select";
            let (offset, choices) = branches(input, plural, depth)?;
            if plural {
                Argument::Plural {
                    choices,
                    offset,
                    ordinal: kind == "selectordinal",
                }
            } else {
                Argument::Select(choices)
            }
        }
        "number" | "date" | "time" => {
            let style = if input.next_if_eq(&',').is_some() {
                take_until(input, &['}'])
            } else {
                String::new()
            };
            match kind.as_str() {
                "number" => Argument::Number(style),
                "date" => Argument::Date(style),
                _ => Argument::Time(style),
            }
        }
        _ => return None,
    };
    input.next_if_eq(&'}')?;
    Some(Node::Argument(key, value))
}

fn branches(input: &mut Input<'_>, plural: bool, depth: usize) -> Option<(f64, Choices)> {
    let mut choices = Vec::new();
    let mut offset = 0.0;
    while let Some(character) = input.peek() {
        if *character == '}' {
            break;
        }
        skip_space(input);
        if input.peek() == Some(&'}') {
            break;
        }
        let mut key = String::new();
        while let Some(character) =
            input.next_if(|ch| !ch.is_whitespace() && *ch != '{' && *ch != '}')
        {
            key.push(character);
            if plural && key == "offset:" {
                break;
            }
        }
        if plural && key == "offset:" && choices.is_empty() {
            skip_space(input);
            let mut amount = String::new();
            while let Some(character) = input.next_if(|ch| !ch.is_whitespace()) {
                amount.push(character);
            }
            offset = amount
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())?;
            continue;
        }
        skip_space(input);
        input.next_if_eq(&'{')?;
        if key.is_empty() {
            return None;
        }
        choices.push((key, message(input, true, plural, depth)?));
    }
    choices
        .iter()
        .any(|(key, _)| key == "other")
        .then_some((offset, choices))
}

fn skip_space(input: &mut Input<'_>) {
    while input.next_if(|ch| ch.is_whitespace()).is_some() {}
}

fn take_until(input: &mut Input<'_>, stop: &[char]) -> String {
    let mut buffer = String::new();
    while let Some(character) = input.next_if(|ch| !stop.contains(ch)) {
        buffer.push(character);
    }
    buffer.trim().to_owned()
}
