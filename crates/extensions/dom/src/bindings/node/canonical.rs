//! PHP method arguments for C14N and its asynchronous file variant.
//!
//! ```php
//! $root->C14NFile('canonical.xml', withComments: true);
//! ```

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, Value, args,
};

use crate::serialize::{self, CanonicalOptions};
use crate::tree::Arena;
use crate::xpath;

use super::super::{This, document::io, this_node};

pub(super) fn c14n(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    canonicalize(context, this, arguments, 0, "DOMNode::C14N")
}

pub(super) fn c14n_file<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        const METHOD: &str = "DOMNode::C14NFile";
        let (_, path) = io::filename(context, &arguments, METHOD)?;
        match canonicalize(context, this.as_ref(), &arguments, 1, METHOD)? {
            Value::String(xml) => match tokio::fs::write(path, xml.as_bytes()).await {
                Ok(()) => Ok(Value::Int(xml.as_bytes().len() as i64)),
                Err(_) => Ok(Value::Bool(false)),
            },
            value => Ok(value),
        }
    })
}

fn canonicalize(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    offset: usize,
    method: &str,
) -> NativeResult {
    let node = this_node(this)?;
    let exclusive = args::bool(arguments, offset).unwrap_or_default();
    let with_comments = args::bool(arguments, offset + 1).unwrap_or_default();
    let prefixes = args::array(arguments, offset + 3);
    let mut include = Vec::new();
    if let Some(prefixes) = prefixes {
        if exclusive {
            for (_, prefix) in prefixes.iter() {
                if let Value::String(prefix) = &*prefix.deref() {
                    include.push(String::from_utf8_lossy(prefix.as_bytes()).into_owned());
                }
            }
        } else {
            context.notice(format!(
                "{method}(): Inclusive namespace prefixes only allowed in exclusive mode."
            ));
        }
    }
    let state = node.shared.borrow();
    if !state.owned {
        return Ok(Value::Bool(false));
    }
    let document = &state.document;
    let mut selection = None;
    if let Some(query) = args::array(arguments, offset + 2) {
        let number = 3 + offset;
        let query_value = query.get(&ArrayKey::str("query")).ok_or_else(|| {
            NativeError::value_error(format!(
                "{method}(): Argument #{number} ($xpath) must have a \"query\" key"
            ))
        })?;
        let query_value = query_value.deref();
        let Value::String(expression) = &*query_value else {
            return Err(NativeError::type_error(format!(
                "{method}(): Argument #{number} ($xpath) \"query\" option must be a string, {} given",
                query_value.type_name(),
            )));
        };
        let mut namespaces = HashMap::new();
        if let Some(value) = query.get(&ArrayKey::str("namespaces"))
            && let Value::Array(entries) = &*value.deref()
        {
            for (key, value) in entries.iter() {
                if let (ArrayKey::String(key), Value::String(uri)) = (key, &*value.deref()) {
                    namespaces.insert(
                        String::from_utf8_lossy(key.as_bytes()).into_owned(),
                        String::from_utf8_lossy(uri.as_bytes()).into_owned(),
                    );
                }
            }
        }
        let result = xpath::evaluate(
            &document.arena,
            node.id,
            &namespaces,
            &String::from_utf8_lossy(expression.as_bytes()),
        );
        match result {
            Ok(xpath::Value::Nodes(nodes)) => {
                selection = Some(nodes.into_iter().collect::<HashSet<_>>());
            }
            Ok(_) => return Err(NativeError::error("XPath query did not return a nodeset")),
            Err(error) => {
                drop(state);
                context.warning(format!("{method}(): {}", error.message()));
                return Ok(Value::Bool(false));
            }
        }
    }
    let root = if selection.is_some() {
        Arena::DOCUMENT
    } else {
        node.id
    };
    Ok(Value::from(serialize::canonical(
        document,
        root,
        CanonicalOptions {
            exclusive,
            with_comments,
            selection: selection.as_ref(),
            prefixes: &include,
        },
    )))
}
