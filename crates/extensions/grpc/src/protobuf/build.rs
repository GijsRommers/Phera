//! Turns message descriptions, from a descriptor set or a PHP array, into
//! the flat tables of a [`Schema`]. Names are resolved here, once: the
//! finished schema refers to messages only by index.

use phera_extension_api::ArrayKey;

use super::wire::MAX_FIELD_NUMBER;
use super::{DIRECT, FieldDef, Kind, MessageDef, MethodDef, Schema, flags};

/// A field before its type name is resolved.
#[derive(Debug, Clone, Default)]
pub struct FieldSpec {
    pub name: String,
    pub number: u32,
    /// `None` when only a type name is known: a message or an enum.
    pub kind: Option<Kind>,
    /// For message (and enum) fields: absolute (`.pkg.Msg`) or relative to
    /// the message that declares the field, as in a `.proto` file.
    pub type_name: Option<String>,
    pub repeated: bool,
    pub packed: bool,
    /// See [`flags::PRESENCE`]; message fields always have it.
    pub presence: bool,
    pub utf8: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MessageSpec {
    /// Full name without a leading dot.
    pub name: String,
    pub fields: Vec<FieldSpec>,
    pub map_entry: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MethodSpec {
    /// The full service name, `pkg.Service`.
    pub service: String,
    pub name: String,
    pub input: String,
    pub output: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

#[derive(Debug, Default)]
pub struct SchemaBuilder {
    pub messages: Vec<MessageSpec>,
    /// Full names of enums, to tell an enum field from a message field when
    /// a descriptor gives only the type name.
    pub enums: Vec<String>,
    pub methods: Vec<MethodSpec>,
}

impl SchemaBuilder {
    pub fn finish(self) -> Result<Schema, String> {
        let SchemaBuilder {
            messages: specs,
            mut enums,
            methods: method_specs,
        } = self;
        if specs.len() >= u32::MAX as usize {
            return Err("too many messages".into());
        }
        let mut by_name: Vec<u32> = (0..specs.len() as u32).collect();
        by_name.sort_by(|&a, &b| specs[a as usize].name.cmp(&specs[b as usize].name));
        if let Some(pair) = by_name
            .windows(2)
            .find(|pair| specs[pair[0] as usize].name == specs[pair[1] as usize].name)
        {
            return Err(format!(
                "message {} is defined twice",
                specs[pair[0] as usize].name
            ));
        }
        enums.sort();
        let names = Names {
            specs: &specs,
            by_name: &by_name,
            enums: &enums,
        };

        let mut messages = Vec::with_capacity(specs.len());
        let mut fields = Vec::with_capacity(specs.iter().map(|spec| spec.fields.len()).sum());
        for spec in &specs {
            let first_field = fields.len();
            let mut own: Vec<&FieldSpec> = spec.fields.iter().collect();
            own.sort_by_key(|field| field.number);
            if own.len() >= usize::from(u16::MAX) {
                return Err(format!("message {} has too many fields", spec.name));
            }
            let mut direct = [0u16; DIRECT];
            for (local, field) in own.iter().enumerate() {
                let path = || format!("{}.{}", spec.name, field.name);
                if field.number == 0 || field.number > MAX_FIELD_NUMBER {
                    return Err(format!(
                        "field {} has invalid number {}",
                        path(),
                        field.number
                    ));
                }
                if local > 0 && own[local - 1].number == field.number {
                    return Err(format!(
                        "fields {}.{} and {} share number {}",
                        spec.name,
                        own[local - 1].name,
                        path(),
                        field.number
                    ));
                }
                if field.name.is_empty() {
                    return Err(format!(
                        "field {} of {} has no name",
                        field.number, spec.name
                    ));
                }
                if own[..local].iter().any(|other| other.name == field.name) {
                    return Err(format!("field {} is defined twice", path()));
                }
                let (kind, message) = names
                    .resolve_field(&spec.name, field)
                    .map_err(|error| format!("field {}: {error}", path()))?;
                let mut bits = 0u8;
                if field.repeated {
                    bits |= flags::REPEATED;
                    if field.packed && kind.is_packable() {
                        bits |= flags::PACKED;
                    }
                    if kind == Kind::Message && specs[message as usize].map_entry {
                        check_map_entry(&specs[message as usize])
                            .map_err(|error| format!("field {}: {error}", path()))?;
                        bits |= flags::MAP;
                    }
                } else if field.presence || kind == Kind::Message {
                    bits |= flags::PRESENCE;
                }
                if field.utf8 && kind == Kind::String {
                    bits |= flags::UTF8;
                }
                if let Some(slot) = direct.get_mut((field.number - 1) as usize) {
                    *slot = local as u16 + 1;
                }
                fields.push(FieldDef {
                    number: field.number,
                    message,
                    kind,
                    flags: bits,
                    key: ArrayKey::interned(field.name.as_bytes()),
                });
            }
            messages.push(MessageDef {
                name: spec.name.clone().into_boxed_str(),
                first_field: first_field as u32,
                field_count: own.len() as u32,
                direct,
                map_entry: spec.map_entry,
            });
        }

        let mut methods = Vec::with_capacity(method_specs.len());
        for spec in &method_specs {
            let path = format!("/{}/{}", spec.service, spec.name);
            let resolve = |name: &str| {
                names
                    .message(&spec.service, name)
                    .ok_or_else(|| format!("method {path}: unknown message type {name}"))
            };
            methods.push(MethodDef {
                input: resolve(&spec.input)?,
                output: resolve(&spec.output)?,
                path: path.into_boxed_str(),
                client_streaming: spec.client_streaming,
                server_streaming: spec.server_streaming,
            });
        }
        methods.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(pair) = methods.windows(2).find(|pair| pair[0].path == pair[1].path) {
            return Err(format!("method {} is defined twice", pair[0].path));
        }

        Ok(Schema {
            messages,
            fields,
            by_name,
            methods,
        })
    }
}

/// A map entry is `{ K key = 1; V value = 2; }` with a scalar key.
fn check_map_entry(entry: &MessageSpec) -> Result<(), String> {
    let field = |number| entry.fields.iter().find(|field| field.number == number);
    let (Some(key), Some(_)) = (field(1), field(2)) else {
        return Err(format!("map entry {} needs fields 1 and 2", entry.name));
    };
    match key.kind {
        Some(kind) if kind.is_map_key() => Ok(()),
        _ => Err(format!("map entry {} has an invalid key type", entry.name)),
    }
}

struct Names<'a> {
    specs: &'a [MessageSpec],
    by_name: &'a [u32],
    enums: &'a [String],
}

impl Names<'_> {
    fn resolve_field(&self, scope: &str, field: &FieldSpec) -> Result<(Kind, u32), String> {
        match (field.kind, field.type_name.as_deref()) {
            (Some(Kind::Message), Some(name)) | (None, Some(name)) => {
                if let Some(id) = self.message(scope, name) {
                    return Ok((Kind::Message, id));
                }
                if self.is_enum(scope, name) {
                    return Ok((Kind::Enum, 0));
                }
                Err(format!("unknown type {name}"))
            }
            (Some(Kind::Message), None) => Err("a message field needs a message type".into()),
            (Some(kind), _) => Ok((kind, 0)),
            (None, None) => Err("no type".into()),
        }
    }

    /// Looks `name` up as a `.proto` file would from inside `scope`: in
    /// `scope`, then in each enclosing scope; a leading dot is absolute.
    fn message(&self, scope: &str, name: &str) -> Option<u32> {
        candidates(scope, name).find_map(|candidate| {
            self.by_name
                .binary_search_by(|&id| self.specs[id as usize].name.as_str().cmp(&candidate))
                .ok()
                .map(|position| self.by_name[position])
        })
    }

    fn is_enum(&self, scope: &str, name: &str) -> bool {
        candidates(scope, name).any(|candidate| self.enums.binary_search(&candidate).is_ok())
    }
}

fn candidates<'a>(scope: &'a str, name: &'a str) -> impl Iterator<Item = String> + 'a {
    let absolute = name.strip_prefix('.');
    let mut scope = Some(scope);
    std::iter::from_fn(move || {
        if let Some(absolute) = absolute {
            return scope.take().map(|_| absolute.to_owned());
        }
        let current = scope?;
        scope = if current.is_empty() {
            None
        } else {
            Some(current.rsplit_once('.').map_or("", |(outer, _)| outer))
        };
        Some(if current.is_empty() {
            name.to_owned()
        } else {
            format!("{current}.{name}")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_names_resolve_from_the_inside_out() {
        let found: Vec<String> = candidates("pkg.Outer", "Inner").collect();
        assert_eq!(found, ["pkg.Outer.Inner", "pkg.Inner", "Inner"]);
        let found: Vec<String> = candidates("pkg.Outer", ".other.Type").collect();
        assert_eq!(found, ["other.Type"]);
    }
}
