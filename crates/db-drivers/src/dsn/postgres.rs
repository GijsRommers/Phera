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

/// Parse libpq's PostgreSQL URL form, such as `pg_connect('postgres://host/app')`.
pub fn uri_fields(source: &str) -> Result<BTreeMap<String, String>, String> {
    let url = url::Url::parse(source).map_err(|error| error.to_string())?;
    let text = |value: &str| {
        percent_encoding::percent_decode_str(value)
            .decode_utf8_lossy()
            .into_owned()
    };
    let mut fields = BTreeMap::new();
    if let Some(host) = url.host_str() {
        fields.insert("host".into(), text(host.trim_matches(['[', ']'])));
    }
    if let Some(port) = url.port() {
        fields.insert("port".into(), port.to_string());
    }
    if !url.username().is_empty() {
        fields.insert("user".into(), text(url.username()));
    }
    if let Some(password) = url.password() {
        fields.insert("password".into(), text(password));
    }
    if let Some(path) = url.path().strip_prefix('/').filter(|path| !path.is_empty()) {
        fields.insert("dbname".into(), text(path));
    }
    for (name, value) in url.query_pairs() {
        fields.insert(name.into_owned(), value.into_owned());
    }
    Ok(fields)
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
