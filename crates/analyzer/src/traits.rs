//! Trait adaptations: `use T { original as alias; }` adds a second method name.
//! Linking the owned index also covers anonymous and nested class declarations.

use crate::index::{Index, MemberAdaptation};

pub fn link(index: &mut Index) {
    for _ in 0..256 {
        let classes: Vec<_> = index
            .classes
            .iter()
            .map(|(name, c)| (name.clone(), c.adaptations.clone()))
            .collect();
        let mut changed = false;
        for (class_name, adaptations) in classes {
            for adaptation in adaptations {
                let (source, method, alias, visibility) = match adaptation {
                    MemberAdaptation::Alias {
                        source,
                        method,
                        alias,
                        visibility,
                    } => (
                        source.unwrap_or_else(|| class_name.clone()),
                        method,
                        alias,
                        visibility,
                    ),
                    MemberAdaptation::Precedence { source, method } => {
                        (source, method.clone(), method, None)
                    }
                };
                let Some(mut signature) = index.method(&source, &method).cloned() else {
                    continue;
                };
                if let Some(visibility) = visibility {
                    signature.modifiers.visibility = Some(visibility);
                }
                if let Some(class) = index.classes.get_mut(&class_name)
                    && !class.methods.contains_key(&alias)
                {
                    class.methods.insert(alias, signature);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}
