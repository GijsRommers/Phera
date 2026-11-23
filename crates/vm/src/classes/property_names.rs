//! Find instance declarations by their case-sensitive PHP name. Private
//! declarations with the same name keep their original, root-first order.
use super::Class;
use super::class::PropertyDefault;
use phera_bytecode::Visibility;

/// Wide classes keep a sorted index into their existing declarations. Small
/// classes scan their declarations directly and need no separate allocation.
#[derive(Default)]
pub(crate) struct PropertyNames(Box<[usize]>);

impl PropertyNames {
    pub(super) fn new(defaults: &[PropertyDefault]) -> Self {
        if defaults.len() <= 8 {
            return Self::default();
        }
        let mut indices: Vec<_> = (0..defaults.len()).collect();
        indices.sort_by(|&a, &b| defaults[a].name.cmp(&defaults[b].name));
        Self(indices.into_boxed_slice())
    }
}

impl Class {
    pub(crate) fn instance_property_named(&self, name: &str, scope: Option<&str>) -> Option<usize> {
        let indices = &self.property_names.0;
        if indices.is_empty() {
            if scope.is_none() {
                return self
                    .defaults
                    .iter()
                    .rposition(|property| property.name == name);
            }
            return select(
                self.defaults
                    .iter()
                    .enumerate()
                    .filter(|(_, property)| property.name == name),
                scope,
            );
        }
        let first = indices.partition_point(|&i| self.defaults[i].name.as_str() < name);
        let matches = indices[first..]
            .iter()
            .map(|&i| (i, &self.defaults[i]))
            .take_while(|(_, property)| property.name == name);
        select(matches, scope)
    }
}

/// A lexical private declaration wins. Otherwise PHP uses the last declaration.
fn select<'a>(
    matches: impl Iterator<Item = (usize, &'a PropertyDefault)>,
    scope: Option<&str>,
) -> Option<usize> {
    let mut last = None;
    for (position, property) in matches {
        if property.visibility == Visibility::Private && Some(property.declaring.as_str()) == scope
        {
            return Some(position);
        }
        last = Some(position);
    }
    last
}
