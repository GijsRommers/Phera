//! Load only the local schema files explicitly requested for validation.
//!
//! ```php
//! $document->schemaValidateSource($schemaWithLocalImports);
//! ```

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tokio::io::AsyncReadExt;

use crate::document::ParseError;
use crate::tree::{Arena, Kind};
use crate::xml;

use super::{Schema, SchemaFile, XSD, attr, elements, issue};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 32;

pub(crate) enum Input<'a> {
    File(PathBuf),
    Source(&'a [u8], PathBuf),
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum Namespace {
    Import(String),
    Include(String),
}

struct Pending<'a> {
    input: Input<'a>,
    namespace: Option<Namespace>,
}

pub(crate) async fn load(input: Input<'_>) -> Result<Schema, Vec<ParseError>> {
    let mut result = Schema::default();
    let mut remaining = MAX_BYTES;
    let mut seen = HashSet::new();
    let mut pending = vec![Pending {
        input,
        namespace: None,
    }];
    while let Some(Pending { input, namespace }) = pending.pop() {
        let (bytes, directory) = match input {
            Input::Source(bytes, directory) => (Cow::Borrowed(bytes), directory),
            Input::File(path) => {
                let canonical = tokio::fs::canonicalize(&path)
                    .await
                    .map_err(|_| missing(&path))?;
                if !seen.insert((canonical.clone(), namespace.clone())) {
                    continue;
                }
                let bytes = Cow::Owned(read(&canonical, remaining).await?);
                let directory = canonical
                    .parent()
                    .map_or_else(PathBuf::new, Path::to_path_buf);
                (bytes, directory)
            }
        };
        if bytes.len() > remaining || result.files.len() >= MAX_FILES {
            return Err(invalid("Schema resource limit exceeded."));
        }
        remaining -= bytes.len();
        let parsed = xml::parse(&bytes, xml::Options::default());
        if !parsed.diagnostics.is_empty() {
            return Err(parsed.diagnostics);
        }
        let document = parsed
            .document
            .ok_or_else(|| invalid("The schema is empty."))?;
        let arena = document.arena;
        if arena
            .children(Arena::DOCUMENT)
            .any(|id| arena[id].kind == Kind::Doctype)
        {
            return Err(invalid("Schema documents cannot contain a doctype."));
        }
        let Some(root) = elements(&arena, Arena::DOCUMENT).next().filter(|&root| {
            arena[root].name.namespace.as_deref() == Some(XSD) && arena[root].name.local == "schema"
        }) else {
            return Err(invalid("The XML document is not a schema document."));
        };
        let declared = attr(&arena, root, "targetNamespace").unwrap_or_default();
        let namespace = match namespace {
            Some(Namespace::Include(namespace)) if declared.is_empty() => namespace,
            Some(Namespace::Include(namespace) | Namespace::Import(namespace))
                if declared != namespace =>
            {
                return Err(invalid(
                    "The schema target namespace does not match its reference.",
                ));
            }
            _ => declared.into(),
        };
        for node in elements(&arena, root) {
            if arena[node].name.namespace.as_deref() != Some(XSD) {
                continue;
            }
            let referenced = match arena[node].name.local.as_str() {
                "import" => Namespace::Import(attr(&arena, node, "namespace").unwrap_or("").into()),
                "include" => Namespace::Include(namespace.clone()),
                "redefine" | "override" => {
                    return Err(invalid("Schema redefinition is unsupported."));
                }
                _ => continue,
            };
            let Some(location) = attr(&arena, node, "schemaLocation") else {
                continue;
            };
            if matches!(referenced, Namespace::Include(_))
                && location.bytes().any(|byte| byte.is_ascii_whitespace())
            {
                return Err(vec![
                    issue(3069, 0, "Internal error: xmlSchemaParseIncludeOrRedefine, could not build an URI from the schemaLocation.".into()),
                    issue(3069, 0, "Internal error: xmlSchemaParse, An internal error occurred.".into()),
                ]);
            }
            let path = reference_path(&directory, location)
                .ok_or_else(|| invalid("Schema resources must be local files."))?;
            if pending.len() >= MAX_FILES {
                return Err(invalid("Schema resource limit exceeded."));
            }
            pending.push(Pending {
                input: Input::File(path),
                namespace: Some(referenced),
            });
        }
        result.add(SchemaFile {
            root,
            namespace,
            elements_qualified: attr(&arena, root, "elementFormDefault") == Some("qualified"),
            attributes_qualified: attr(&arena, root, "attributeFormDefault") == Some("qualified"),
            arena,
        });
    }
    Ok(result)
}

pub(crate) fn local_path(directory: &Path, location: &str) -> Option<PathBuf> {
    if location.is_empty() || location.contains('\0') {
        return None;
    }
    if let Ok(url) = url::Url::parse(location) {
        return file_path(url);
    }
    Some(directory.join(location))
}

fn reference_path(directory: &Path, location: &str) -> Option<PathBuf> {
    if location.is_empty() || location.contains('\0') {
        return None;
    }
    let base = url::Url::from_directory_path(directory).ok()?;
    file_path(base.join(location).ok()?)
}

fn file_path(url: url::Url) -> Option<PathBuf> {
    if url.scheme() == "file" && url.query().is_none() && url.fragment().is_none() {
        url.to_file_path().ok()
    } else {
        None
    }
}

async fn read(path: &Path, remaining: usize) -> Result<Vec<u8>, Vec<ParseError>> {
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
        .await
        .map_err(|_| missing(path))?;
    if !file.metadata().await.is_ok_and(|meta| meta.is_file()) {
        return Err(invalid("Schema resources must be regular files."));
    }
    let mut bytes = Vec::new();
    file.take(remaining as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| missing(path))?;
    Ok(bytes)
}

fn missing(path: &Path) -> Vec<ParseError> {
    vec![issue(
        1757,
        0,
        format!(
            "Failed to locate the schema resource at '{}'.",
            path.display()
        ),
    )]
}

fn invalid(message: &str) -> Vec<ParseError> {
    vec![issue(1757, 0, message.into())]
}
