//! Which class extends which, across every file of a build:
//! `final class Invoice extends Document implements Countable`.
//!
//! ## How to read the tables
//!
//! Classes are sorted by lowercased name, and [`AotClassLayout::name`] is
//! that lowercased name. Besides every class the build declares, the table
//! holds each name a declaration extends or implements, so that a parent is
//! always a [`ClassRef`].
//!
//! `decl` says how much is known about an entry:
//!
//! - `Unique`: one declaration, and the entry describes it.
//! - `Duplicate`: several declarations share the name. The entry holds what
//!   is true whichever of them is loaded: a flag that proves something
//!   (`FINAL`) needs all of them, a flag that warns (`MAGIC_GET`) needs one.
//! - `Native` and `Unknown`: a class of the runtime, or one that no file of
//!   the build declares. Nothing is known about its own parents or members,
//!   so every warning flag is set and its ancestors are empty.
//!
//! `ancestors` and the descendants are *may* relations: `B` is listed under
//! `A` when some declaration of `B` can make it an `A`. They are exact when
//! every class on the way is `Unique`. Two consequences:
//!
//! - A class with a `Native` or `Unknown` ancestor has more ancestors than
//!   listed (`RuntimeException` is also an `Exception`), and the descendants
//!   of a native class miss those that reach it through another native
//!   class. The runtime's own hierarchy is not part of the build's input.
//! - The descendants of a class the build declares are complete: no class of
//!   the build can extend it without being listed.
//!
//! `DYNAMIC_PROPS` means the class asks for dynamic properties
//! (`#[AllowDynamicProperties]`, also when inherited). Its absence does not
//! mean an instance never gets one: PHP 8.5 still creates an undeclared
//! property on assignment, with a deprecation notice, unless the class is
//! `readonly`.
//!
//! `methods` holds what the class body itself declares. Inherited and trait
//! methods are not copied in; `slots` stays empty until layouts are computed.

use std::collections::{BTreeMap, BTreeSet};

use phera_bytecode::facts::{
    AotClassLayout, AotMethod, ClassFlags, ClassRef, DeclUniqueness, FunctionRef, MethodFlags,
    UnitRef,
};
use phera_bytecode::{
    ClassDeclaration, ClassKind, CompiledUnit, MethodDeclaration, TraitAdaptation, Visibility,
};

use super::CompiledWorldUnit;
use super::declarations::Declarations;

/// The flags that warn about a class: what its instances may do behind a
/// plain-looking property access or call.
const HAZARDS: ClassFlags = ClassFlags(
    ClassFlags::DYNAMIC_PROPS.0
        | ClassFlags::MAGIC_GET.0
        | ClassFlags::MAGIC_SET.0
        | ClassFlags::MAGIC_CALL.0
        | ClassFlags::MAGIC_CALL_STATIC.0
        | ClassFlags::MAGIC_DESTRUCT.0,
);

/// The class table of a build and the relations between its entries.
pub(crate) struct Hierarchy {
    pub(crate) classes: Box<[AotClassLayout]>,
    /// The descendants of every class, one run after another.
    pub(crate) descendants: Box<[ClassRef]>,
    /// For each unit, the table entry of each of its declarations, by
    /// `ClassId`.
    pub(crate) unit_classes: Vec<Box<[ClassRef]>>,
    /// Per class: every declaration is a class that can be instantiated.
    concrete: Vec<bool>,
    /// The runtime's reflection classes that the build mentions, and the
    /// build's classes that extend one.
    reflection: BTreeSet<String>,
}

/// One declaration of a class, reduced to what the hierarchy needs. Names
/// are lowercased.
struct Declared {
    kind: ClassKind,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    parent: Option<String>,
    /// What a class implements, or what an interface extends.
    interfaces: Vec<String>,
    traits: Vec<String>,
    /// The hazards of the class body itself.
    hazards: ClassFlags,
    declares_to_string: bool,
    methods: Vec<AotMethod>,
}

/// What a class has through its own body, its traits and its parents.
#[derive(Clone, Copy)]
struct Members {
    hazards: ClassFlags,
    to_string: bool,
}

/// The build's own declarations by lowercased class name. A name the
/// runtime provides is absent even when a file declares it too.
struct OwnDeclarations {
    declared: BTreeMap<String, Vec<Declared>>,
}

impl OwnDeclarations {
    fn members(&self, name: &str) -> Members {
        let mut members = Members {
            hazards: ClassFlags::NONE,
            to_string: false,
        };
        let mut visited = BTreeSet::new();
        let mut pending = vec![name];
        while let Some(name) = pending.pop() {
            if !visited.insert(name) {
                continue;
            }
            let Some(declared) = self.declared.get(name) else {
                // A class or trait the build does not declare: what it
                // brings is not known.
                members.hazards = members.hazards.union(HAZARDS);
                continue;
            };
            for declaration in declared {
                members.hazards = members.hazards.union(declaration.hazards);
                members.to_string |= declaration.declares_to_string;
                pending.extend(declaration.parent.as_deref());
                pending.extend(declaration.traits.iter().map(String::as_str));
            }
        }
        members
    }

    /// The names a class can be an instance of, one step up.
    fn supertypes(&self, name: &str) -> BTreeSet<String> {
        let mut supertypes = BTreeSet::new();
        let Some(declared) = self.declared.get(name) else {
            return supertypes;
        };
        for declaration in declared {
            supertypes.extend(declaration.parent.iter().cloned());
            supertypes.extend(declaration.interfaces.iter().cloned());
        }
        // PHP makes every class with a `__toString` method a `Stringable`.
        let is_type = declared
            .iter()
            .any(|declaration| declaration.kind != ClassKind::Trait);
        if is_type && self.members(name).to_string {
            supertypes.insert("stringable".to_owned());
        }
        supertypes.remove(name);
        supertypes
    }
}

impl Hierarchy {
    pub(crate) fn build(
        units: &[CompiledWorldUnit],
        declarations: &Declarations<'_>,
        native_supertypes: &BTreeMap<String, Vec<String>>,
    ) -> Self {
        let mut own = OwnDeclarations {
            declared: BTreeMap::new(),
        };
        for (name, sites) in &declarations.classes {
            let by_the_build = matches!(
                declarations.class(name),
                DeclUniqueness::Unique { .. } | DeclUniqueness::Duplicate
            );
            if !by_the_build {
                continue;
            }
            let declared = sites
                .iter()
                .filter_map(|site| {
                    let unit = &units.get(site.unit as usize)?.unit;
                    let declaration = unit.classes.get(site.class.index())?;
                    Some(describe(site.unit, unit, declaration))
                })
                .collect();
            own.declared.insert(name.clone(), declared);
        }

        // Every name of the table: what the build declares and what those
        // declarations extend or implement.
        let mut supertypes: BTreeMap<String, BTreeSet<String>> = declarations
            .classes
            .keys()
            .map(|name| (name.clone(), own.supertypes(name)))
            .collect();
        let mentioned: Vec<String> = supertypes.values().flatten().cloned().collect();
        for name in mentioned {
            supertypes.entry(name).or_default();
        }
        // Pull in relevant runtime/native and bundled-PHP hierarchy edges to a
        // fixed point (e.g. `Phera\Symfony\Runtime extends SymfonyRuntime`, or
        // `RuntimeException extends Exception extends Throwable`) so sealed
        // builds never falsely mark a runtime-subclassed type EFFECTIVELY_FINAL.
        loop {
            let mut changed = false;
            for (child, parents) in native_supertypes {
                if own.declared.contains_key(child) {
                    continue;
                }
                let touches_world = supertypes.contains_key(child)
                    || parents.iter().any(|parent| supertypes.contains_key(parent));
                if !touches_world {
                    continue;
                }
                let entry = supertypes.entry(child.clone()).or_default();
                let before = entry.len();
                for parent in parents {
                    if parent != child {
                        entry.insert(parent.clone());
                    }
                }
                if entry.len() != before {
                    changed = true;
                }
                for parent in parents {
                    if !supertypes.contains_key(parent) {
                        supertypes.insert(parent.clone(), BTreeSet::new());
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let names: Vec<&str> = supertypes.keys().map(String::as_str).collect();
        let class_ref = |name: &str| names.binary_search(&name).ok().map(|i| ClassRef(i as u32));

        let above: Vec<Vec<ClassRef>> = names
            .iter()
            .map(|name| {
                supertypes[*name]
                    .iter()
                    .filter_map(|n| class_ref(n))
                    .collect()
            })
            .collect();
        let ancestors: Vec<Vec<ClassRef>> = (0..names.len())
            .map(|class| ancestors_of(class, &above))
            .collect();
        let mut below: Vec<Vec<ClassRef>> = vec![Vec::new(); names.len()];
        for (class, ancestors) in ancestors.iter().enumerate() {
            for ancestor in ancestors {
                below[ancestor.0 as usize].push(ClassRef(class as u32));
            }
        }

        let kinds = kinds_of_mentioned(&own);
        let mut classes = Vec::with_capacity(names.len());
        let mut concrete = Vec::with_capacity(names.len());
        let mut descendants = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let declared = own
                .declared
                .get(*name)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let every =
                |test: fn(&Declared) -> bool| !declared.is_empty() && declared.iter().all(test);
            let mut flags = own.members(name).hazards;
            if every(|declaration| declaration.is_final) {
                flags = flags.union(ClassFlags::FINAL);
            }
            if every(|declaration| declaration.is_abstract) {
                flags = flags.union(ClassFlags::ABSTRACT);
            }
            if every(|declaration| declaration.is_readonly) {
                flags = flags.union(ClassFlags::READONLY);
            }
            concrete.push(every(|declaration| {
                declaration.kind.is_class() && !declaration.is_abstract
            }));
            let first = declared.first();
            // Several declarations have a parent only when they agree on it.
            let parent = first
                .and_then(|first| first.parent.as_deref())
                .filter(|parent| {
                    declared
                        .iter()
                        .all(|other| other.parent.as_deref() == Some(*parent))
                })
                .and_then(class_ref);
            let methods = match declared {
                [only] => only.methods.clone(),
                _ => Vec::new(),
            };
            classes.push(AotClassLayout {
                name: (*name).into(),
                decl: declarations.class(name),
                kind: first
                    .map(|first| first.kind)
                    .or_else(|| kinds.get(*name).copied())
                    .unwrap_or(ClassKind::Class),
                parent,
                ancestors: ancestors[index].clone().into_boxed_slice(),
                first_descendant: descendants.len() as u32,
                descendants: below[index].len() as u32,
                flags,
                slots: Box::default(),
                methods: methods.into_boxed_slice(),
            });
            descendants.extend_from_slice(&below[index]);
        }

        let reflection = names
            .iter()
            .enumerate()
            .filter(|(index, name)| {
                let is_native_reflection =
                    |name: &str| named_like_reflection(name) && !own.declared.contains_key(name);
                is_native_reflection(name)
                    || ancestors[*index]
                        .iter()
                        .any(|ancestor| is_native_reflection(names[ancestor.0 as usize]))
            })
            .map(|(_, name)| (*name).to_owned())
            .collect();
        let unit_classes = units
            .iter()
            .map(|unit| {
                unit.unit
                    .classes
                    .iter()
                    .map(|declaration| {
                        class_ref(&declaration.name.to_ascii_lowercase()).unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        Self {
            classes: classes.into_boxed_slice(),
            descendants: descendants.into_boxed_slice(),
            unit_classes,
            concrete,
            reflection,
        }
    }

    /// Marks the classes that no class extends: the ones declared `final`,
    /// and, when no subclass can appear at run time, the ones that have no
    /// descendant in the build.
    pub(crate) fn mark_effectively_final(&mut self, sealed_hierarchy: bool) {
        for (layout, concrete) in self.classes.iter_mut().zip(&self.concrete) {
            let closed = layout.flags.contains(ClassFlags::FINAL)
                || (sealed_hierarchy && layout.descendants == 0);
            if *concrete && closed {
                layout.flags = layout.flags.union(ClassFlags::EFFECTIVELY_FINAL);
            }
        }
    }

    pub(crate) fn class_ref(&self, lowercased: &str) -> Option<ClassRef> {
        self.classes
            .binary_search_by(|layout| (*layout.name).cmp(lowercased))
            .ok()
            .map(|index| ClassRef(index as u32))
    }

    pub(crate) fn descendants_of(&self, class: ClassRef) -> &[ClassRef] {
        let Some(layout) = self.classes.get(class.0 as usize) else {
            return &[];
        };
        let start = layout.first_descendant as usize;
        let end = start.saturating_add(layout.descendants as usize);
        self.descendants.get(start..end).unwrap_or(&[])
    }

    /// Whether a lowercased class name is one of the runtime's reflection
    /// classes, or a class of the build that extends one.
    pub(crate) fn is_reflection(&self, lowercased: &str) -> bool {
        self.reflection.contains(lowercased)
            || (named_like_reflection(lowercased) && self.class_ref(lowercased).is_none())
    }
}

/// `ReflectionClass`, `ReflectionMethod`, ...: the global classes whose name
/// starts with `Reflection`. The exception they throw inspects nothing.
fn named_like_reflection(lowercased: &str) -> bool {
    lowercased.starts_with("reflection")
        && !lowercased.contains('\\')
        && lowercased != "reflectionexception"
}

/// Everything reachable upwards from `class`, itself excluded. A cycle in
/// broken code (`A extends B`, `B extends A`) ends at the class it began at.
fn ancestors_of(class: usize, above: &[Vec<ClassRef>]) -> Vec<ClassRef> {
    let mut seen = BTreeSet::new();
    let mut pending = above[class].clone();
    while let Some(ancestor) = pending.pop() {
        if ancestor.0 as usize != class && seen.insert(ancestor) {
            pending.extend_from_slice(&above[ancestor.0 as usize]);
        }
    }
    seen.into_iter().collect()
}

/// Whether a name the build only mentions is used as a class or as an
/// interface: `extends` of a class names a class, everything else an
/// interface.
fn kinds_of_mentioned(own: &OwnDeclarations) -> BTreeMap<String, ClassKind> {
    let mut kinds = BTreeMap::new();
    for declaration in own.declared.values().flatten() {
        if let Some(parent) = &declaration.parent {
            kinds.insert(parent.clone(), ClassKind::Class);
        }
        for interface in &declaration.interfaces {
            kinds
                .entry(interface.clone())
                .or_insert(ClassKind::Interface);
        }
        if declaration.declares_to_string {
            kinds
                .entry("stringable".to_owned())
                .or_insert(ClassKind::Interface);
        }
    }
    kinds
}

fn describe(unit_index: u32, unit: &CompiledUnit, declaration: &ClassDeclaration) -> Declared {
    let class_name = |symbol: phera_bytecode::SymbolId| {
        unit.symbols
            .get(symbol.index())
            .map(|symbol| class_key(&symbol.name))
    };
    let mut interfaces: Vec<String> = declaration
        .interfaces
        .iter()
        .filter_map(|&interface| class_name(interface))
        .collect();
    if declaration.kind == ClassKind::Enum {
        // Every enum is a `UnitEnum`; one with a backing type is also a
        // `BackedEnum`. The declaration does not spell either.
        interfaces.push("unitenum".to_owned());
        if declaration.enum_backing.is_some() {
            interfaces.push("backedenum".to_owned());
        }
    }

    let mut hazards = ClassFlags::NONE;
    if declaration.allows_dynamic_properties {
        hazards = hazards.union(ClassFlags::DYNAMIC_PROPS);
    }
    let mut declares_to_string = false;
    let mut methods = Vec::with_capacity(declaration.methods.len());
    for method in &declaration.methods {
        let Some(name) = unit.names.get(method.name.index()) else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        hazards = hazards.union(magic_hazard(&name));
        declares_to_string |= name == "__tostring";
        methods.push(AotMethod {
            name: name.into(),
            function: FunctionRef {
                unit: UnitRef(unit_index),
                function: method.function.0 + 1,
            },
            flags: method_flags(method),
        });
    }
    methods.sort_by(|a, b| a.name.cmp(&b.name));
    // `use Helpers { read as __get; }` gives a trait method a magic name.
    for adaptation in declaration.traits.iter().flat_map(|used| &used.adaptations) {
        if let TraitAdaptation::Alias {
            alias: Some(alias), ..
        } = adaptation
        {
            let alias = alias.to_ascii_lowercase();
            hazards = hazards.union(magic_hazard(&alias));
            declares_to_string |= alias == "__tostring";
        }
    }

    Declared {
        kind: declaration.kind,
        is_final: declaration.is_final,
        is_abstract: declaration.is_abstract,
        is_readonly: declaration.is_readonly,
        parent: declaration.parent.and_then(class_name),
        interfaces,
        traits: declaration
            .traits
            .iter()
            .flat_map(|used| &used.names)
            .map(|name| class_key(name))
            .collect(),
        hazards,
        declares_to_string,
        methods,
    }
}

/// What a method of this lowercased name lets its class do behind a plain
/// property access or call. `isset($o->x)` and `unset($o->x)` are answered
/// by `__isset` and `__unset`, which count as reading and writing overloads.
fn magic_hazard(method: &str) -> ClassFlags {
    match method {
        "__get" | "__isset" => ClassFlags::MAGIC_GET,
        "__set" | "__unset" => ClassFlags::MAGIC_SET,
        "__call" => ClassFlags::MAGIC_CALL,
        "__callstatic" => ClassFlags::MAGIC_CALL_STATIC,
        "__destruct" => ClassFlags::MAGIC_DESTRUCT,
        _ => ClassFlags::NONE,
    }
}

fn method_flags(method: &MethodDeclaration) -> MethodFlags {
    let mut flags = match method.visibility {
        Visibility::Public => MethodFlags::PUBLIC,
        Visibility::Protected => MethodFlags::PROTECTED,
        Visibility::Private => MethodFlags::PRIVATE,
    };
    if method.is_static {
        flags = flags.union(MethodFlags::STATIC);
    }
    if method.is_final {
        flags = flags.union(MethodFlags::FINAL);
    }
    if method.is_abstract {
        flags = flags.union(MethodFlags::ABSTRACT);
    }
    flags
}

/// A class name as the table keys it: no leading backslash, lowercased.
fn class_key(name: &str) -> String {
    name.strip_prefix('\\').unwrap_or(name).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::Hierarchy;
    use crate::sound::declarations::Declarations;
    use crate::sound::test_support::{class, method, symbol, unit_of, world_unit};
    use phera_bytecode::facts::{
        AotClassLayout, ClassFlags, ClassRef, DeclUniqueness, FunctionRef, MethodFlags, UnitRef,
    };
    use phera_bytecode::{
        ClassKind, CompiledUnit, EnumBacking, FunctionId, SymbolId, TraitAdaptation, TraitUse,
        Visibility,
    };
    use std::collections::BTreeSet;

    fn build(units: Vec<CompiledUnit>, natives: &[&str], sealed: bool) -> Hierarchy {
        build_with_edges(units, natives, std::collections::BTreeMap::new(), sealed)
    }

    fn build_with_edges(
        units: Vec<CompiledUnit>,
        natives: &[&str],
        native_supertypes: std::collections::BTreeMap<String, Vec<String>>,
        sealed: bool,
    ) -> Hierarchy {
        let units: Vec<_> = units
            .into_iter()
            .enumerate()
            .map(|(index, unit)| world_unit(&format!("{index}.php"), unit))
            .collect();
        let native_functions = BTreeSet::new();
        let native_classes: BTreeSet<String> = natives.iter().map(|n| (*n).to_owned()).collect();
        let declarations = Declarations::collect(&units, &native_functions, &native_classes);
        let mut hierarchy = Hierarchy::build(&units, &declarations, &native_supertypes);
        hierarchy.mark_effectively_final(sealed);
        hierarchy
    }

    #[test]
    fn runtime_shipped_subclass_prevents_effectively_final() {
        let mut unit = unit_of(Vec::new());
        unit.classes = vec![class("Symfony\\Component\\Runtime\\SymfonyRuntime", None)];
        let edges = std::collections::BTreeMap::from([(
            "phera\\symfony\\runtime".to_owned(),
            vec!["symfony\\component\\runtime\\symfonyruntime".to_owned()],
        )]);
        let hierarchy = build_with_edges(vec![unit], &["phera\\symfony\\runtime"], edges, true);
        let symfony = layout(&hierarchy, "symfony\\component\\runtime\\symfonyruntime");
        assert!(
            !symfony.flags.contains(ClassFlags::EFFECTIVELY_FINAL),
            "runtime-subclassed vendor class must not be EFFECTIVELY_FINAL"
        );
        assert_eq!(
            descendants(&hierarchy, "symfony\\component\\runtime\\symfonyruntime"),
            ["phera\\symfony\\runtime"]
        );
    }

    fn layout<'a>(hierarchy: &'a Hierarchy, name: &str) -> &'a AotClassLayout {
        let class = hierarchy.class_ref(name).expect(name);
        &hierarchy.classes[class.0 as usize]
    }

    fn names(hierarchy: &Hierarchy, classes: &[ClassRef]) -> Vec<String> {
        classes
            .iter()
            .map(|class| hierarchy.classes[class.0 as usize].name.to_string())
            .collect()
    }

    fn descendants(hierarchy: &Hierarchy, name: &str) -> Vec<String> {
        let layout = layout(hierarchy, name);
        let start = layout.first_descendant as usize;
        let end = start + layout.descendants as usize;
        names(hierarchy, &hierarchy.descendants[start..end])
    }

    /// interface Shape extends Countable {}
    /// abstract class Base implements Shape { function __get() }
    /// class Circle extends Base {}
    /// final class Square extends Base { function area() }
    /// class Loose {}
    fn shapes() -> CompiledUnit {
        let mut unit = unit_of(vec![]);
        unit.symbols = vec![
            symbol("Countable", None),
            symbol("App\\Shape", None),
            symbol("App\\Base", None),
        ];
        unit.names = vec!["__get".into(), "area".into()];
        let mut shape = class("App\\Shape", None);
        shape.kind = ClassKind::Interface;
        shape.interfaces = vec![SymbolId(0)];
        let mut base = class("App\\Base", None);
        base.is_abstract = true;
        base.interfaces = vec![SymbolId(1)];
        base.methods = vec![method(0, 0)];
        let circle = class("App\\Circle", Some(SymbolId(2)));
        let mut square = class("App\\Square", Some(SymbolId(2)));
        square.is_final = true;
        square.methods = vec![method(1, 1)];
        square.methods[0].visibility = Visibility::Protected;
        square.methods[0].is_static = true;
        unit.classes = vec![shape, base, circle, square, class("Loose", None)];
        unit
    }

    #[test]
    fn classes_are_sorted_and_linked_to_ancestors_and_descendants() {
        let hierarchy = build(vec![shapes()], &["countable"], false);
        let all: Vec<&str> = hierarchy.classes.iter().map(|c| &*c.name).collect();
        assert_eq!(
            all,
            [
                "app\\base",
                "app\\circle",
                "app\\shape",
                "app\\square",
                "countable",
                "loose"
            ]
        );

        let square = layout(&hierarchy, "app\\square");
        assert_eq!(
            square.decl,
            DeclUniqueness::Unique {
                unit: UnitRef(0),
                hoisted: false
            }
        );
        assert_eq!(square.parent, hierarchy.class_ref("app\\base"));
        assert_eq!(
            names(&hierarchy, &square.ancestors),
            ["app\\base", "app\\shape", "countable"]
        );
        assert_eq!(descendants(&hierarchy, "app\\square"), Vec::<String>::new());
        assert_eq!(
            descendants(&hierarchy, "app\\base"),
            ["app\\circle", "app\\square"]
        );
        assert_eq!(
            descendants(&hierarchy, "countable"),
            ["app\\base", "app\\circle", "app\\shape", "app\\square"]
        );

        let countable = layout(&hierarchy, "countable");
        assert_eq!(countable.decl, DeclUniqueness::Native);
        assert_eq!(countable.kind, ClassKind::Interface);
        assert!(countable.ancestors.is_empty());
        // Each unit's declarations point at their entries, by class id.
        assert_eq!(
            names(&hierarchy, &hierarchy.unit_classes[0]),
            [
                "app\\shape",
                "app\\base",
                "app\\circle",
                "app\\square",
                "loose"
            ]
        );
    }

    #[test]
    fn flags_come_from_the_declaration_and_what_it_inherits() {
        let hierarchy = build(vec![shapes()], &["countable"], false);
        let base = layout(&hierarchy, "app\\base").flags;
        assert!(base.contains(ClassFlags::ABSTRACT));
        assert!(base.contains(ClassFlags::MAGIC_GET));
        assert!(!base.contains(ClassFlags::MAGIC_SET));
        // Circle inherits __get from Base.
        let circle = layout(&hierarchy, "app\\circle").flags;
        assert!(circle.contains(ClassFlags::MAGIC_GET));
        assert!(!circle.contains(ClassFlags::ABSTRACT));
        // Final is closed whether or not the hierarchy is sealed.
        let square = layout(&hierarchy, "app\\square").flags;
        assert!(square.contains(ClassFlags::FINAL));
        assert!(square.contains(ClassFlags::EFFECTIVELY_FINAL));
        // Without a seal a subclass of Circle or Loose may still be loaded.
        assert!(!circle.contains(ClassFlags::EFFECTIVELY_FINAL));
        let loose = layout(&hierarchy, "loose").flags;
        assert_eq!(loose, ClassFlags::NONE);
        // Nothing is known about a class of the runtime.
        let countable = layout(&hierarchy, "countable").flags;
        assert!(countable.contains(ClassFlags::MAGIC_GET));
        assert!(countable.contains(ClassFlags::DYNAMIC_PROPS));
        assert!(!countable.contains(ClassFlags::EFFECTIVELY_FINAL));

        let methods = &layout(&hierarchy, "app\\square").methods;
        assert_eq!(methods.len(), 1);
        assert_eq!(&*methods[0].name, "area");
        assert_eq!(
            methods[0].function,
            FunctionRef {
                unit: UnitRef(0),
                function: 2
            }
        );
        assert_eq!(
            methods[0].flags,
            MethodFlags::PROTECTED.union(MethodFlags::STATIC)
        );
    }

    #[test]
    fn a_sealed_hierarchy_closes_concrete_classes_without_descendants() {
        let hierarchy = build(vec![shapes()], &["countable"], true);
        let closed = |name: &str| {
            layout(&hierarchy, name)
                .flags
                .contains(ClassFlags::EFFECTIVELY_FINAL)
        };
        assert!(closed("app\\circle"));
        assert!(closed("app\\square"));
        assert!(closed("loose"));
        // Abstract, an interface, or extended by another class of the build.
        assert!(!closed("app\\base"));
        assert!(!closed("app\\shape"));
        assert!(!closed("countable"));
    }

    #[test]
    fn traits_bring_their_magic_and_unknown_parents_bring_every_warning() {
        let mut unit = unit_of(vec![]);
        unit.symbols = vec![symbol("Vendor\\Missing", None)];
        unit.names = vec!["__call".into(), "__toString".into(), "__destruct".into()];
        let mut inner = class("App\\Inner", None);
        inner.kind = ClassKind::Trait;
        inner.methods = vec![method(0, 0), method(1, 1)];
        let mut outer = class("App\\Outer", None);
        outer.kind = ClassKind::Trait;
        outer.traits = vec![TraitUse {
            names: vec!["App\\Inner".into()],
            adaptations: Vec::new(),
        }];
        let mut user = class("App\\User", None);
        user.traits = vec![TraitUse {
            names: vec!["\\App\\Outer".into()],
            adaptations: Vec::new(),
        }];
        user.methods = vec![method(2, 2)];
        user.allows_dynamic_properties = true;
        // class Renamer { use Inner { helper as __set; } }
        let mut renamer = class("App\\Renamer", None);
        renamer.traits = vec![TraitUse {
            names: vec!["App\\Inner".into()],
            adaptations: vec![TraitAdaptation::Alias {
                trait_name: None,
                method: "helper".into(),
                alias: Some("__SET".into()),
                visibility: None,
                is_final: false,
            }],
        }];
        let orphan = class("App\\Orphan", Some(SymbolId(0)));
        unit.classes = vec![inner, outer, user, orphan, renamer];
        let hierarchy = build(vec![unit], &[], true);
        let renamer = layout(&hierarchy, "app\\renamer").flags;
        assert!(renamer.contains(ClassFlags::MAGIC_SET));
        assert!(renamer.contains(ClassFlags::MAGIC_CALL));
        assert!(!renamer.contains(ClassFlags::MAGIC_GET));

        let user = layout(&hierarchy, "app\\user");
        assert!(user.flags.contains(ClassFlags::MAGIC_CALL));
        assert!(user.flags.contains(ClassFlags::MAGIC_DESTRUCT));
        assert!(user.flags.contains(ClassFlags::DYNAMIC_PROPS));
        assert!(!user.flags.contains(ClassFlags::MAGIC_GET));
        // __toString came through the traits: the class is a Stringable.
        assert_eq!(names(&hierarchy, &user.ancestors), ["stringable"]);
        assert_eq!(
            layout(&hierarchy, "stringable").decl,
            DeclUniqueness::Unknown
        );
        // A trait is not a type, so it is nobody's ancestor and no Stringable.
        assert!(layout(&hierarchy, "app\\inner").ancestors.is_empty());
        assert!(
            !layout(&hierarchy, "app\\inner")
                .flags
                .contains(ClassFlags::EFFECTIVELY_FINAL)
        );

        let orphan = layout(&hierarchy, "app\\orphan");
        assert_eq!(names(&hierarchy, &orphan.ancestors), ["vendor\\missing"]);
        assert!(orphan.flags.contains(ClassFlags::MAGIC_GET));
        assert!(orphan.flags.contains(ClassFlags::MAGIC_SET));
        assert!(orphan.flags.contains(ClassFlags::MAGIC_CALL_STATIC));
        let missing = layout(&hierarchy, "vendor\\missing");
        assert_eq!(missing.decl, DeclUniqueness::Unknown);
        assert_eq!(missing.kind, ClassKind::Class);
    }

    #[test]
    fn duplicate_declarations_keep_only_what_all_of_them_agree_on() {
        // a.php: final class Twice extends One { function __set() }
        // b.php: class Twice extends Two {}
        // c.php: class Child extends Twice {}
        let mut a = unit_of(vec![]);
        a.symbols = vec![symbol("One", None)];
        a.names = vec!["__set".into()];
        let mut first = class("Twice", Some(SymbolId(0)));
        first.is_final = true;
        first.methods = vec![method(0, 0)];
        a.classes = vec![first, class("One", None)];
        let mut b = unit_of(vec![]);
        b.symbols = vec![symbol("Two", None)];
        b.classes = vec![class("twice", Some(SymbolId(0))), class("Two", None)];
        let mut c = unit_of(vec![]);
        c.symbols = vec![symbol("Twice", None)];
        c.classes = vec![class("Child", Some(SymbolId(0)))];
        let hierarchy = build(vec![a, b, c], &[], true);

        let twice = layout(&hierarchy, "twice");
        assert_eq!(twice.decl, DeclUniqueness::Duplicate);
        assert_eq!(twice.parent, None);
        assert!(twice.methods.is_empty());
        assert!(!twice.flags.contains(ClassFlags::FINAL));
        assert!(twice.flags.contains(ClassFlags::MAGIC_SET));
        assert!(!twice.flags.contains(ClassFlags::EFFECTIVELY_FINAL));
        // Either parent may be the one at run time.
        assert_eq!(names(&hierarchy, &twice.ancestors), ["one", "two"]);
        let child = layout(&hierarchy, "child");
        assert_eq!(names(&hierarchy, &child.ancestors), ["one", "twice", "two"]);
        assert!(child.flags.contains(ClassFlags::MAGIC_SET));
        assert!(child.flags.contains(ClassFlags::EFFECTIVELY_FINAL));
        assert_eq!(descendants(&hierarchy, "one"), ["child", "twice"]);
        assert!(
            !layout(&hierarchy, "one")
                .flags
                .contains(ClassFlags::EFFECTIVELY_FINAL)
        );
    }

    #[test]
    fn enums_cycles_and_runtime_names_are_handled() {
        // enum Suit: string {}   enum Flag {}
        // class A extends B {}   class B extends A {}
        // if (!interface_exists('Stringable')) { interface Stringable {} }
        let mut unit = unit_of(vec![]);
        unit.symbols = vec![symbol("B", None), symbol("A", None)];
        let mut suit = class("Suit", None);
        suit.kind = ClassKind::Enum;
        suit.is_final = true;
        suit.enum_backing = Some(EnumBacking::String);
        let mut flag = class("Flag", None);
        flag.kind = ClassKind::Enum;
        flag.is_final = true;
        let mut polyfill = class("Stringable", None);
        polyfill.kind = ClassKind::Interface;
        unit.classes = vec![
            suit,
            flag,
            class("A", Some(SymbolId(0))),
            class("B", Some(SymbolId(1))),
            polyfill,
        ];
        let hierarchy = build(vec![unit], &["stringable", "unitenum", "backedenum"], false);

        let suit = layout(&hierarchy, "suit");
        assert_eq!(
            names(&hierarchy, &suit.ancestors),
            ["backedenum", "unitenum"]
        );
        assert!(suit.flags.contains(ClassFlags::EFFECTIVELY_FINAL));
        assert_eq!(descendants(&hierarchy, "unitenum"), ["flag", "suit"]);
        assert_eq!(descendants(&hierarchy, "backedenum"), ["suit"]);

        assert_eq!(names(&hierarchy, &layout(&hierarchy, "a").ancestors), ["b"]);
        assert_eq!(names(&hierarchy, &layout(&hierarchy, "b").ancestors), ["a"]);

        // The runtime's interface wins over the polyfill of the same name.
        let stringable = layout(&hierarchy, "stringable");
        assert_eq!(stringable.decl, DeclUniqueness::Native);
        assert!(stringable.methods.is_empty());
        assert_eq!(
            names(&hierarchy, &hierarchy.unit_classes[0][4..]),
            ["stringable"]
        );
    }

    #[test]
    fn reflection_is_the_runtime_family_and_what_extends_it() {
        // class Mirror extends ReflectionClass {}   class Deep extends Mirror {}
        // class ReflectionHelper {}   class Plain {}
        let mut unit = unit_of(vec![]);
        unit.symbols = vec![symbol("ReflectionClass", None), symbol("Mirror", None)];
        unit.functions = vec![crate::sound::test_support::function(vec![])];
        unit.hoisted = vec![FunctionId(0)];
        unit.classes = vec![
            class("Mirror", Some(SymbolId(0))),
            class("Deep", Some(SymbolId(1))),
            class("ReflectionHelper", None),
            class("Plain", None),
        ];
        let hierarchy = build(vec![unit], &["reflectionclass"], false);
        assert!(hierarchy.is_reflection("reflectionclass"));
        assert!(hierarchy.is_reflection("mirror"));
        assert!(hierarchy.is_reflection("deep"));
        // Never mentioned by a declaration, but named like the family.
        assert!(hierarchy.is_reflection("reflectionmethod"));
        assert!(!hierarchy.is_reflection("reflectionexception"));
        assert!(!hierarchy.is_reflection("app\\reflectionthing"));
        // The build's own class of that name is ordinary code.
        assert!(!hierarchy.is_reflection("reflectionhelper"));
        assert!(!hierarchy.is_reflection("plain"));
    }
}
