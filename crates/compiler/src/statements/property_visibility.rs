//! `public private(set) int $id` is publicly readable and only its class may write it.

use phera_ast::{Modifiers, Visibility as SourceVisibility};
use phera_bytecode::Visibility;
use phera_source::Span;

use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(super) fn property_write_visibility(
        &self,
        modifiers: &Modifiers,
        location: (&str, &str, Span),
        ty: Option<&str>,
        readonly: bool,
    ) -> Result<(Option<Visibility>, bool), CompileError> {
        let (class, name, span) = location;
        let line = || self.lines.line(span.start);
        let read = modifiers.visibility.unwrap_or(SourceVisibility::Public);
        let write = modifiers.set_visibility;
        if let Some(ty) = ty
            && ty.split(['|', '&', '?']).any(|name| name == "callable")
        {
            return Err(self.error(
                format!("Property {class}::${name} cannot have type {ty}"),
                line(),
            ));
        }
        if readonly && modifiers.is_static {
            return Err(self.error(
                format!("Static property {class}::${name} cannot be readonly"),
                line(),
            ));
        }
        if readonly && ty.is_none() {
            return Err(self.error(
                format!("Readonly property {class}::${name} must have type"),
                line(),
            ));
        }
        if write.is_some() && ty.is_none() {
            return Err(self.error(
                format!("Property with asymmetric visibility {class}::${name} must have type"),
                line(),
            ));
        }
        if write.is_some_and(|write| level(read) > level(write)) {
            return Err(self.error(
                format!(
                    "Visibility of property {class}::${name} must not be weaker than set visibility"
                ),
                line(),
            ));
        }
        if modifiers.is_final && read == SourceVisibility::Private {
            return Err(self.error("Property cannot be both final and private", line()));
        }
        let restricted = match (read, write) {
            (read, Some(write)) if write == read => None,
            (_, Some(SourceVisibility::Protected)) => Some(Visibility::Protected),
            (_, Some(SourceVisibility::Private)) => Some(Visibility::Private),
            (SourceVisibility::Public, None) if readonly => Some(Visibility::Protected),
            _ => None,
        };
        Ok((
            restricted,
            modifiers.is_final || restricted == Some(Visibility::Private),
        ))
    }

    pub(super) fn reject_member_set_visibility(
        &self,
        modifiers: &Modifiers,
        member: &str,
        span: Span,
    ) -> Result<(), CompileError> {
        if let Some(write) = modifiers.set_visibility {
            let visibility = match write {
                SourceVisibility::Public => "public",
                SourceVisibility::Protected => "protected",
                SourceVisibility::Private => "private",
            };
            return Err(self.error(
                format!("Cannot use the {visibility}(set) modifier on a {member}"),
                self.lines.line(span.start),
            ));
        }
        Ok(())
    }
}

fn level(visibility: SourceVisibility) -> u8 {
    match visibility {
        SourceVisibility::Public => 0,
        SourceVisibility::Protected => 1,
        SourceVisibility::Private => 2,
    }
}
