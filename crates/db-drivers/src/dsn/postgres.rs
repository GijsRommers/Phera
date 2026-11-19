//! PDO PostgreSQL accepts libpq keyword syntax, such as `dbname='my database'`.

use std::collections::BTreeMap;
use std::iter::Peekable;

use super::c_space;

pub fn fields(input: &str, pdo: bool) -> Result<BTreeMap<String, String>, String> {
    let mut input = input
        .chars()
        .map(|ch| if pdo && ch == ';' { ' ' } else { ch })
        .peekable();
    let mut fields = BTreeMap::new();
    while input.peek().is_some() {
        skip_space(&mut input);
        if input.peek().is_none() {
            break;
        }
        let mut key = String::new();
        while let Some(ch) = input.next_if(|ch| *ch != '=' && !c_space(*ch)) {
            key.push(ch);
        }
        skip_space(&mut input);
        if input.next() != Some('=') {
            return Err(format!(
                "missing \"=\" after \"{key}\" in connection info string"
            ));
        }
        skip_space(&mut input);
        let quoted = input.next_if_eq(&'\'').is_some();
        let mut value = String::new();
        let mut closed = !quoted;
        while let Some(ch) = input.next_if(|ch| quoted || !c_space(*ch)) {
            match ch {
                '\\' => {
                    if let Some(ch) = input.next() {
                        value.push(ch);
                    }
                }
                '\'' if quoted => {
                    closed = true;
                    break;
                }
                _ => value.push(ch),
            }
        }
        if !closed {
            return Err("unterminated quoted string in connection info string".into());
        }
        fields.insert(key, value);
    }
    Ok(fields)
}

fn skip_space(input: &mut Peekable<impl Iterator<Item = char>>) {
    while input.next_if(|ch| c_space(*ch)).is_some() {}
}

#[cfg(test)]
mod tests {
    use super::fields;

    #[test]
    fn parses_laravel_style_quotes_and_libpq_escapes() {
        let options = fields(
            "host=localhost;dbname='my database'; application_name='a\\'b'",
            true,
        )
        .unwrap();
        assert_eq!(options["dbname"], "my database");
        assert_eq!(options["application_name"], "a'b");
        assert_eq!(options["host"], "localhost");
        assert!(fields("dbname='unterminated", true).is_err());
    }
}
