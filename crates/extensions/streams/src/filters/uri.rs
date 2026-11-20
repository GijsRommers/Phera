//! `php://filter/read=string.rot13/resource=report.txt` opens a filtered stream.

use percent_encoding::percent_decode;
use phera_extension_api::{Context, NativeError, Value};

use super::{Direction, Filter, Filters, MAX_STAGES};

pub(crate) struct FilterUri<'a> {
    pub path: &'a [u8],
    groups: Vec<Vec<(&'a [u8], Option<Direction>)>>,
}

pub(crate) fn is_filter_uri(path: &[u8]) -> bool {
    after(path, b"php://filter/").is_some()
}

impl<'a> FilterUri<'a> {
    pub fn parse(mut path: &'a [u8]) -> Result<Option<Self>, &'static str> {
        let mut groups = Vec::new();
        let mut stages = 0;
        let mut depth = 0;
        while let Some(uri) = after(path, b"php://filter/") {
            depth += 1;
            if depth > MAX_STAGES {
                return Err("Stream filter chain exceeds Phera's limit of 16 filters");
            }
            let Some(resource) = after(uri, b"resource=").map(|_| (0, 9)).or_else(|| {
                uri.windows(10)
                    .position(|part| part.eq_ignore_ascii_case(b"/resource="))
                    .map(|index| (index, index + 10))
            }) else {
                return Err("No URL resource specified");
            };
            let mut layer = Vec::new();
            for group in uri[..resource.0]
                .split(|byte| *byte == b'/')
                .filter(|group| !group.is_empty())
            {
                let (chain, direction) = if let Some(chain) = after(group, b"read=") {
                    (chain, Some(Direction::Read))
                } else if let Some(chain) = after(group, b"write=") {
                    (chain, Some(Direction::Write))
                } else {
                    (group, None)
                };
                stages += chain.split(|byte| *byte == b'|').count();
                if stages > MAX_STAGES {
                    return Err("Stream filter chain exceeds Phera's limit of 16 filters");
                }
                layer.push((chain, direction));
            }
            groups.push(layer);
            path = &uri[resource.1..];
        }
        Ok((depth != 0).then_some(Self { path, groups }))
    }

    fn names(&self, direction: Direction, mode: &[u8]) -> Vec<Vec<u8>> {
        let mut names = Vec::new();
        let bare = match direction {
            Direction::Read => mode.contains(&b'r') || mode.contains(&b'+'),
            Direction::Write => mode.contains(&b'+') || mode.first() != Some(&b'r'),
        };
        for index in 0..self.groups.len() {
            let layer = match direction {
                Direction::Read => self.groups.len() - 1 - index,
                Direction::Write => index,
            };
            for &(group, explicit) in &self.groups[layer] {
                if explicit != Some(direction) && !(bare && explicit.is_none()) {
                    continue;
                }
                for name in group
                    .split(|byte| *byte == b'|')
                    .filter(|name| !name.is_empty())
                {
                    names.push(percent_decode(name).collect());
                }
            }
        }
        names
    }

    pub fn build(&self, context: &mut dyn Context, function: &str, mode: &[u8]) -> Filters {
        let mut filters = Filters::default();
        let iconv = context.iconv_converter();
        for (direction, chain) in [
            (Direction::Read, &mut filters.read),
            (Direction::Write, &mut filters.write),
        ] {
            for name in self.names(direction, mode) {
                if let Some(filter) = Filter::new(0, &name, &Value::Null, iconv) {
                    chain.add(filter, false);
                } else {
                    super::functions::cannot_create(context, function, &name);
                    cannot_create_uri(context, function, &name);
                }
            }
        }
        filters
    }
}

/// A filter URI needs the request when a stage or its backing stream can run PHP.
pub(crate) async fn open_request_filter(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    mode: &[u8],
    include_path: bool,
    stream_context: &Value,
) -> Result<Option<Value>, NativeError> {
    use super::user::{self, Placement};
    use crate::user_wrappers;

    if after(name, b"php://filter/").is_none()
        || user_wrappers::locate(context, name).is_some()
        || user_wrappers::protocol_disabled(context, b"php")
    {
        return Ok(None);
    }
    let Some(uri) = FilterUri::parse(name).map_err(NativeError::error)? else {
        return Ok(None);
    };
    let read = uri.names(Direction::Read, mode);
    let write = uri.names(Direction::Write, mode);
    let needs_request = user_wrappers::locate(context, uri.path).is_some()
        || read
            .iter()
            .chain(&write)
            .any(|name| user::class(context, name).is_some());
    if !needs_request {
        return Ok(None);
    }
    let stream = crate::handles::open_resource(
        context,
        function,
        uri.path,
        mode,
        include_path,
        stream_context,
    )
    .await?;
    let Value::Resource(resource) = &stream else {
        return Ok(Some(stream));
    };
    let result = async {
        for (names, directions) in [(read, (true, false)), (write, (false, true))] {
            for name in names {
                if let Some(class) = user::class(context, &name) {
                    let created = user::attach(
                        context,
                        resource,
                        function,
                        &name,
                        &class,
                        &Value::Null,
                        (directions, Placement::Uri),
                    )
                    .await?;
                    if !matches!(created, Value::Bool(true)) {
                        cannot_create_uri(context, function, &name);
                    }
                } else if let Some(filter) =
                    Filter::new(0, &name, &Value::Null, context.iconv_converter())
                {
                    let (read, write) = if directions.0 {
                        (Some(filter), None)
                    } else {
                        (None, Some(filter))
                    };
                    user::attach_native(context, resource, read, write, Placement::Uri)?;
                } else {
                    super::functions::cannot_create(context, function, &name);
                    cannot_create_uri(context, function, &name);
                }
            }
        }
        Ok::<_, NativeError>(())
    }
    .await;
    if let Err(error) = result {
        let _ = crate::handles::close_arguments(context, vec![stream], function).await;
        return Err(error);
    }
    Ok(Some(stream))
}

fn cannot_create_uri(context: &mut dyn Context, function: &str, name: &[u8]) {
    let name = String::from_utf8_lossy(name);
    context.warning(format!("{function}(): Unable to create filter ({name})"));
}

fn after<'a>(path: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    path.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &path[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_rejects_excessive_stages_before_opening_the_resource() {
        let chain = ["string.rot13"; MAX_STAGES + 1].join("|");
        let uri = format!("php://filter/read={chain}/resource=not-opened");
        assert!(FilterUri::parse(uri.as_bytes()).is_err());
    }

    #[test]
    fn url_rejects_excessive_nesting_even_if_the_chains_are_empty() {
        let uri = "php://filter/resource=".repeat(MAX_STAGES + 1) + "not-opened";
        assert!(FilterUri::parse(uri.as_bytes()).is_err());
    }
}
