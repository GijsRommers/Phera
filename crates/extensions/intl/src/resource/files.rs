//! Load resource directories asynchronously, including their declared locale index.
//!
//! ```php
//! ResourceBundle::getLocales(__DIR__ . '/compiled');
//! ```

use std::path::{Path, PathBuf};

use phera_extension_api::{ArrayKey, Context, NativeError, WorkingDirectory};
use tokio::{fs::File, io::AsyncReadExt};

use super::{
    binary,
    data::{Node, Table},
};

const MAX_FILE: u64 = 32 * 1024 * 1024;

pub struct Loaded {
    pub direct: Table,
    pub inheritance: Vec<Table>,
    pub status: i32,
}

pub fn path(context: &mut dyn Context, name: &str) -> Result<PathBuf, NativeError> {
    context
        .state()
        .get_or_default::<WorkingDirectory>()
        .resolve(Path::new(name))
        .map_err(|error| NativeError::error(error.to_string()))
}

pub async fn open(directory: &Path, locale: &str, fallback: bool) -> Option<Loaded> {
    let fallback_chain = locales(locale);
    let mut loaded = None::<Loaded>;
    for (index, locale) in fallback_chain.into_iter().enumerate() {
        if index != 0 && !fallback {
            break;
        }
        let Some(bundle) = read(directory, &locale).await else {
            continue;
        };
        if let Some(loaded) = &mut loaded {
            loaded.inheritance.push(bundle.root);
        } else {
            let status = if index == 0 {
                0
            } else if locale == "root" {
                -127
            } else {
                -128
            };
            loaded = Some(Loaded {
                direct: bundle.root,
                inheritance: Vec::new(),
                status,
            });
        }
        if bundle.no_fallback {
            break;
        }
    }
    loaded
}

pub async fn index(directory: &Path) -> Option<Vec<String>> {
    let bundle = read(directory, "res_index").await?;
    let (_, Node::Table(rows)) = bundle.root.iter().find(
        |(key, _)| matches!(key, ArrayKey::String(key) if key.as_bytes() == b"InstalledLocales"),
    )?
    else {
        return None;
    };
    rows.iter()
        .map(|(key, _)| match key {
            ArrayKey::String(key) => Some(std::str::from_utf8(key.as_bytes()).ok()?.to_owned()),
            _ => None,
        })
        .collect()
}

async fn read(directory: &Path, locale: &str) -> Option<binary::Bundle> {
    let file = File::open(directory.join(format!("{locale}.res")))
        .await
        .ok()?;
    let mut input = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut input).await.ok()?;
    if input.len() as u64 > MAX_FILE {
        return None;
    }
    binary::parse(&input)
}

fn locales(locale: &str) -> Vec<String> {
    let clean = locale.split('@').next().unwrap_or_default();
    if clean.is_empty()
        || clean.len() > crate::locale::MAX_LEN
        || !clean
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_'))
    {
        return Vec::new();
    }
    if clean.eq_ignore_ascii_case("root") {
        return vec!["root".to_owned()];
    }
    let mut name = crate::locale_id::canonical(clean);
    let mut candidates = Vec::new();
    while !name.is_empty() {
        candidates.push(name.clone());
        let Some(index) = name.rfind('_') else { break };
        name.truncate(index);
    }
    candidates.push("root".to_owned());
    candidates
}
