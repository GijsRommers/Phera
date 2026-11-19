//! Trait adaptations: `use T { original as alias; }` adds a second method name.
//! Linking the owned index also covers anonymous and nested class declarations.

use crate::index::{Index, MemberAdaptation};

pub fn link(index: &mut Index) {
    let classes: Vec<_> = index
        .classes
        .iter()
        .filter(|(_, class)| !class.adaptations.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    link_classes(index, &classes);
}

pub fn link_classes(index: &mut Index, names: &[String]) {
    let classes: Vec<_> = names
        .iter()
        .filter_map(|name| {
            let class = index.classes.get(name)?;
            Some((name, class.adaptations.clone()))
        })
        .collect();
    loop {
        let mut changed = false;
        for (class_name, adaptations) in &classes {
            for adaptation in adaptations.iter().cloned() {
                let (source, method, alias, visibility) = match adaptation {
                    MemberAdaptation::Alias {
                        source,
                        method,
                        alias,
                        visibility,
                    } => (source, method, alias, visibility),
                    MemberAdaptation::Precedence { source, method } => {
                        (Some(source), method.clone(), method, None)
                    }
                };
                let signature = if let Some(source) = source {
                    index.method(&source, &method)
                } else {
                    // An unqualified alias names an imported method, even if
                    // the consuming class declares an override with its old name.
                    index.class(class_name).and_then(|class| {
                        class
                            .traits
                            .iter()
                            .find_map(|source| index.method(source, &method))
                    })
                };
                let Some(mut signature) = signature.cloned() else {
                    continue;
                };
                if let Some(visibility) = visibility {
                    signature.modifiers.visibility = Some(visibility);
                }
                if let Some(class) = index.classes.get_mut(*class_name)
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
