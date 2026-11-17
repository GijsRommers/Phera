//! Trait adaptations: `use T { original as alias; }` adds a second method name.
//! Adaptations are linked after collection so a later file may supply the trait.

use crate::{index::Index, names::Names, project::ParsedFile};
use phera_ast::*;

pub fn link(index: &mut Index, files: &[ParsedFile]) {
    for _ in 0..256 {
        let mut changed = false;
        for file in files {
            let mut pending = vec![(
                &file.program.statements[..],
                Names::for_block(String::new(), &file.program.statements),
            )];
            while let Some((block, names)) = pending.pop() {
                for statement in block {
                    match &statement.kind {
                        StatementKind::Namespace {
                            name, statements, ..
                        } => {
                            let namespace = name.as_ref().map_or(String::new(), |n| n.text.clone());
                            pending.push((statements, Names::for_block(namespace, statements)));
                        }
                        StatementKind::Class(class) => changed |= adapt(index, class, &names),
                        _ => {
                            for block in crate::collect::child_blocks(statement) {
                                pending.push((block, names.clone()));
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn adapt(index: &mut Index, class: &ClassDeclaration, names: &Names) -> bool {
    let Some(name) = &class.name else {
        return false;
    };
    let class_name = names.declaration(&name.name);
    let mut changed = false;
    for member in &class.members {
        let ClassMemberKind::TraitUse { adaptations, .. } = &member.kind else {
            continue;
        };
        for adaptation in adaptations {
            match adaptation {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    let source = trait_name.as_ref().map_or(class_name.clone(), |n| {
                        names.resolve(n, UseKind::Class)[0].clone()
                    });
                    let Some(mut signature) = index.method(&source, &method.name).cloned() else {
                        continue;
                    };
                    if let Some(visibility) = visibility {
                        signature.modifiers.visibility = Some(*visibility);
                    }
                    let target = alias.as_ref().unwrap_or(method).name.to_lowercase();
                    if let Some(class) = index.classes.get_mut(&class_name.to_lowercase())
                        && class.methods.get(&target) != Some(&signature)
                    {
                        class.methods.insert(target, signature);
                        changed = true;
                    }
                }
                TraitAdaptation::Precedence {
                    trait_name, method, ..
                } => {
                    let source = names.resolve(trait_name, UseKind::Class)[0].clone();
                    if let Some(signature) = index.method(&source, &method.name).cloned()
                        && let Some(class) = index.classes.get_mut(&class_name.to_lowercase())
                        && !class.methods.contains_key(&method.name.to_lowercase())
                    {
                        class.methods.insert(method.name.to_lowercase(), signature);
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}
