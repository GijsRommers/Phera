//! `php://filter/read=string.rot13/resource=report.txt` opens a filtered stream.

use percent_encoding::percent_decode;
use phera_extension_api::{Context, Value};

use super::{Direction, Filter, Filters, MAX_STAGES};

pub(crate) struct FilterUri<'a> {
    pub path: &'a [u8],
    groups: Vec<Vec<(&'a [u8], Option<Direction>)>>,
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
            let Some(resource) = uri
                .windows(10)
                .position(|part| part.eq_ignore_ascii_case(b"/resource="))
                .map(|index| (index, index + 10))
                .or_else(|| after(uri, b"resource=").map(|_| (0, 9)))
            else {
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

    pub fn build(&self, context: &mut dyn Context, function: &str, mode: &[u8]) -> Filters {
        let mut filters = Filters::default();
        let read = mode.contains(&b'r') || mode.contains(&b'+');
        let write = mode.contains(&b'+') || mode.first() != Some(&b'r');
        for (direction, bare, chain) in [
            (Direction::Read, read, &mut filters.read),
            (Direction::Write, write, &mut filters.write),
        ] {
            let layer = |index| {
                if direction == Direction::Read {
                    self.groups.len() - 1 - index
                } else {
                    index
                }
            };
            for index in 0..self.groups.len() {
                for &(group, explicit) in &self.groups[layer(index)] {
                    if explicit != Some(direction) && !(bare && explicit.is_none()) {
                        continue;
                    }
                    for name in group
                        .split(|byte| *byte == b'|')
                        .filter(|name| !name.is_empty())
                    {
                        let name = percent_decode(name).collect::<Vec<_>>();
                        if let Some(filter) = Filter::new(0, &name, &Value::Null) {
                            chain.add(filter, false);
                        } else {
                            let name = String::from_utf8_lossy(&name);
                            context.warning(format!(
                                "{function}(): Unable to locate filter \"{name}\""
                            ));
                            context
                                .warning(format!("{function}(): Unable to create filter ({name})"));
                        }
                    }
                }
            }
        }
        filters
    }
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
