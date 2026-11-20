//! Require types at declaration sites: `function render($page) {}` is incomplete.

use super::*;
use crate::index::Signature;
use crate::rules::missing_type;

impl Analyzer<'_> {
    pub fn function_hints(&mut self, function: &FunctionDeclaration) {
        if self.level < 6 {
            return;
        }
        let name = self.context.names.declaration(&function.name.name);
        let Some(signature) = self
            .index
            .functions
            .get(&name.to_ascii_lowercase())
            .cloned()
        else {
            return;
        };
        self.signature_hints(
            &signature,
            &format!("Function {name}()"),
            function.name.span,
            &[],
            false,
        );
    }

    pub fn method_hints(&mut self, method: &Method) {
        if self.level < 6 {
            return;
        }
        let Some(class) = self.context.documentation_class.as_deref() else {
            return;
        };
        let Some(owner) = self.index.class(class) else {
            return;
        };
        let name = &method.name.name;
        let key = name.to_ascii_lowercase();
        let Some(mut signature) = owner.methods.get(&key).cloned() else {
            return;
        };
        for (parameter, syntax) in signature.parameters.iter_mut().zip(&method.parameters) {
            if syntax.is_promoted()
                && let Some(documented) = owner
                    .properties
                    .get(&parameter.name)
                    .and_then(|property| property.documented_type.as_ref())
            {
                parameter.ty = documented.clone();
            }
        }
        let inherited: Vec<_> = self
            .index
            .ancestry(class)
            .into_iter()
            .skip(1)
            .filter_map(|base| base.methods.get(&key).cloned())
            .collect();
        let label = format!(
            "Method {}::{name}()",
            self.class_label(class, owner.location.line)
        );
        let magic_return = matches!(
            key.as_str(),
            "__construct" | "__destruct" | "__clone" | "__sleep" | "__serialize"
        );
        self.signature_hints(
            &signature,
            &label,
            method.name.span,
            &inherited,
            magic_return,
        );
    }

    fn signature_hints(
        &mut self,
        signature: &Signature,
        label: &str,
        span: Span,
        inherited: &[Signature],
        magic_return: bool,
    ) {
        for (position, parameter) in signature.parameters.iter().enumerate() {
            let mut ty = parameter_hint(signature, position).unwrap_or(&parameter.ty);
            for parent in inherited {
                let parent_position = parent
                    .parameters
                    .iter()
                    .position(|candidate| candidate.name == parameter.name)
                    .or_else(|| (!label.ends_with("::__construct()")).then_some(position));
                if let Some(parent_ty) =
                    parent_position.and_then(|position| parameter_hint(parent, position))
                {
                    ty = fill_missing_hint(ty, parent_ty);
                }
            }
            let label = format!("{label} has parameter ${}", parameter.name);
            self.report_hints(ty, &label, "missingType.parameter", span);
        }
        if magic_return {
            return;
        }
        let mut ty = return_hint(signature);
        for parent in inherited {
            ty = fill_missing_hint(ty, return_hint(parent));
        }
        self.report_hints(ty, label, "missingType.return", span);
    }

    pub fn property_hints(&mut self, name: &str, span: Span) {
        if self.level < 6 {
            return;
        }
        if let Some(class) = self.context.class.clone()
            && let Some(owner) = self.index.class(&class)
            && let Some(property) = owner.properties.get(name)
        {
            let mut ty = property.documented_type.as_ref().unwrap_or(&property.ty);
            for base in self.index.ancestry(&class).into_iter().skip(1) {
                if let Some(parent) = base.properties.get(name) {
                    ty = fill_missing_hint(
                        ty,
                        parent.documented_type.as_ref().unwrap_or(&parent.ty),
                    );
                }
            }
            let label = self.class_label(&class, owner.location.line);
            self.report_hints(
                &ty.clone(),
                &format!("Property {label}::${name}"),
                "missingType.property",
                span,
            );
        }
    }

    fn class_label(&self, class: &str, line: usize) -> String {
        if !class.starts_with("@anonymous:") {
            return class.into();
        }
        let path = std::path::Path::new(&self.file.path);
        let cwd = std::env::current_dir().ok();
        let path = cwd
            .as_ref()
            .and_then(|cwd| path.strip_prefix(cwd).ok())
            .unwrap_or(path);
        format!("class@anonymous/{}:{line}", path.display())
    }

    fn report_hints(&mut self, ty: &ValueType, label: &str, kind: &'static str, span: Span) {
        for finding in missing_type::hints(ty, label, kind, self.index) {
            self.diagnose(finding.identifier, span, finding.message);
        }
    }
}

fn parameter_hint(signature: &Signature, position: usize) -> Option<&ValueType> {
    let parameter = signature.parameters.get(position)?;
    let annotation = signature
        .documentation
        .as_ref()
        .and_then(|doc| doc.params.get(&parameter.name));
    Some(annotation.map_or(&parameter.ty, |annotation| &annotation.ty))
}

fn return_hint(signature: &Signature) -> &ValueType {
    signature
        .documentation
        .as_ref()
        .and_then(|doc| doc.returns.as_ref())
        .map_or(&signature.returns, |annotation| &annotation.ty)
}

fn fill_missing_hint<'a>(ty: &'a ValueType, inherited: &'a ValueType) -> &'a ValueType {
    use ValueType as T;
    if *ty == T::Mixed {
        return inherited;
    }
    if std::ptr::eq(ty, inherited) || ty == inherited {
        return ty;
    }
    let targets = top_level_types(ty);
    let sources = top_level_types(inherited);
    if targets.iter().any(|target| {
        match target {
        T::Array | T::Iterable => sources.iter().any(|source| {
            matches!(source, T::Collection(_) | T::Shape(_))
        }),
        T::Class(name) => sources.iter().any(|source| {
            matches!(source, T::Generic(generic) if name.eq_ignore_ascii_case(&generic.base))
        }),
        _ => false,
    }
    }) {
        inherited
    } else {
        ty
    }
}

fn top_level_types(ty: &ValueType) -> Vec<&ValueType> {
    let mut pending = vec![ty];
    let mut types = Vec::new();
    while let Some(ty) = pending.pop() {
        match ty {
            ValueType::Union(arms) | ValueType::Intersection(arms) => pending.extend(arms.iter()),
            _ => types.push(ty),
        }
    }
    types
}
