//! Resolve names in `use T { T::m as private alias; }` in the class's namespace.

use phera_ast as ast;
use phera_bytecode::{TraitAdaptation, TraitUse, Visibility};
use phera_source::Span;

use crate::policies::NameKind;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(crate) fn compile_trait_use(
        &self,
        kind: ast::ClassKind,
        class: &str,
        traits: &[ast::Name],
        adaptations: &[ast::TraitAdaptation],
        span: Span,
    ) -> Result<TraitUse, CompileError> {
        let resolve = |name: &ast::Name| {
            if matches!(
                name.text.to_ascii_lowercase().as_str(),
                "self" | "parent" | "static"
            ) {
                return Err(self.error(
                    format!(
                        "Cannot use \"{}\" as trait name, as it is reserved",
                        name.text
                    ),
                    self.lines.line(name.span.start),
                ));
            }
            Ok(self.resolve(name, NameKind::Class).0)
        };
        let names: Vec<_> = traits.iter().map(resolve).collect::<Result<_, _>>()?;
        if kind == ast::ClassKind::Interface
            && let Some(name) = names.first()
        {
            return Err(self.error(
                format!("Cannot use traits inside of interfaces. {name} is used in {class}"),
                self.lines.line(span.start),
            ));
        }
        let adaptations = adaptations
            .iter()
            .map(|adaptation| {
                Ok(match adaptation {
                    ast::TraitAdaptation::Precedence {
                        trait_name,
                        method,
                        instead_of,
                    } => TraitAdaptation::Precedence {
                        trait_name: resolve(trait_name)?,
                        method: method.name.clone(),
                        instead_of: instead_of.iter().map(resolve).collect::<Result<_, _>>()?,
                    },
                    ast::TraitAdaptation::Alias {
                        trait_name,
                        method,
                        alias,
                        visibility,
                        is_final,
                        invalid_modifier,
                    } => {
                        if let Some(modifier) = invalid_modifier {
                            let spelling = modifier.name.to_ascii_lowercase();
                            let message = format!(
                                "Cannot use \"{spelling}\" as method modifier in trait alias"
                            );
                            return Err(self.error(message, self.lines.line(span.start)));
                        }
                        TraitAdaptation::Alias {
                            trait_name: trait_name.as_ref().map(resolve).transpose()?,
                            method: method.name.clone(),
                            alias: alias.as_ref().map(|alias| alias.name.clone()),
                            visibility: visibility.map(|visibility| match visibility {
                                ast::Visibility::Public => Visibility::Public,
                                ast::Visibility::Protected => Visibility::Protected,
                                ast::Visibility::Private => Visibility::Private,
                            }),
                            is_final: *is_final,
                        }
                    }
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(TraitUse { names, adaptations })
    }
}
