//! Which functions and classes the files of a build declare, and how often:
//! `function helper() {}` in two files is one name with two declarations.

use std::collections::{BTreeMap, BTreeSet};

use phera_bytecode::facts::{DeclUniqueness, FunctionRef, UnitRef};
use phera_bytecode::{ClassId, FunctionId, Instruction};

use super::CompiledWorldUnit;

/// One `function name() {}` in the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FunctionSite {
    pub(crate) at: FunctionRef,
    /// Declared unconditionally at the top level of its file, so it exists
    /// before the file's first statement runs.
    pub(crate) hoisted: bool,
}

/// One `class Name {}` in the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClassSite {
    /// Index of the unit among the build's units.
    pub(crate) unit: u32,
    pub(crate) class: ClassId,
    pub(crate) hoisted: bool,
}

/// Every declaration of the build by its lowercased name, in unit order.
pub(crate) struct Declarations<'a> {
    pub(crate) functions: BTreeMap<String, Vec<FunctionSite>>,
    pub(crate) classes: BTreeMap<String, Vec<ClassSite>>,
    native_functions: &'a BTreeSet<String>,
    native_classes: &'a BTreeSet<String>,
}

impl<'a> Declarations<'a> {
    /// `native_functions` and `native_classes` are the lowercased names the
    /// runtime itself provides.
    pub(crate) fn collect(
        units: &[CompiledWorldUnit],
        native_functions: &'a BTreeSet<String>,
        native_classes: &'a BTreeSet<String>,
    ) -> Self {
        let mut functions: BTreeMap<String, Vec<FunctionSite>> = BTreeMap::new();
        let mut classes: BTreeMap<String, Vec<ClassSite>> = BTreeMap::new();
        for (index, unit) in units.iter().enumerate() {
            let index = index as u32;
            let unit = &unit.unit;
            for (function, hoisted) in named_functions(unit) {
                let Some(body) = unit.functions.get(function.index()) else {
                    continue;
                };
                functions
                    .entry(body.name.to_ascii_lowercase())
                    .or_default()
                    .push(FunctionSite {
                        at: FunctionRef {
                            unit: UnitRef(index),
                            function: function.0 + 1,
                        },
                        hoisted,
                    });
            }
            for (class, declaration) in unit.classes.iter().enumerate() {
                let class = ClassId(class as u32);
                classes
                    .entry(declaration.name.to_ascii_lowercase())
                    .or_default()
                    .push(ClassSite {
                        unit: index,
                        class,
                        hoisted: unit.hoisted_classes.contains(&class),
                    });
            }
        }
        Self {
            functions,
            classes,
            native_functions,
            native_classes,
        }
    }

    /// How many declarations a lowercased function name has.
    ///
    /// A name the runtime provides is `Native` even when a file declares it
    /// too: such a declaration is a polyfill behind `function_exists()` and
    /// never runs.
    pub(crate) fn function(&self, name: &str) -> DeclUniqueness {
        if self.native_functions.contains(name) {
            return DeclUniqueness::Native;
        }
        match self.functions.get(name).map(Vec::as_slice) {
            Some([site]) => DeclUniqueness::Unique {
                unit: site.at.unit,
                hoisted: site.hoisted,
            },
            Some([_, _, ..]) => DeclUniqueness::Duplicate,
            Some([]) | None => DeclUniqueness::Unknown,
        }
    }

    /// How many declarations a lowercased class name has; see [`Self::function`].
    pub(crate) fn class(&self, name: &str) -> DeclUniqueness {
        if self.native_classes.contains(name) {
            return DeclUniqueness::Native;
        }
        match self.classes.get(name).map(Vec::as_slice) {
            Some([site]) => DeclUniqueness::Unique {
                unit: UnitRef(site.unit),
                hoisted: site.hoisted,
            },
            Some([_, _, ..]) => DeclUniqueness::Duplicate,
            Some([]) | None => DeclUniqueness::Unknown,
        }
    }

    /// The one declaration of a function, when there is exactly one and the
    /// runtime has no function of that name.
    pub(crate) fn unique_function(&self, name: &str) -> Option<FunctionRef> {
        match self.function(name) {
            DeclUniqueness::Unique { .. } => Some(self.functions.get(name)?.first()?.at),
            _ => None,
        }
    }

    /// The build's functions sorted by lowercased name. A name declared more
    /// than once points at its first declaration.
    pub(crate) fn function_table(&self) -> Box<[(Box<str>, DeclUniqueness, FunctionRef)]> {
        self.functions
            .iter()
            .filter_map(|(name, sites)| {
                let first = sites.first()?;
                Some((name.as_str().into(), self.function(name), first.at))
            })
            .collect()
    }
}

/// The functions a file declares by name, and whether each is hoisted.
/// Methods, closures and initializers are compiled functions too, but only
/// `unit.hoisted` and `DeclareFunction` name a function PHP code can call.
fn named_functions(unit: &phera_bytecode::CompiledUnit) -> BTreeMap<FunctionId, bool> {
    let mut named: BTreeMap<FunctionId, bool> = unit
        .hoisted
        .iter()
        .map(|&function| (function, true))
        .collect();
    let bodies = std::iter::once(&unit.main).chain(&unit.functions);
    for instruction in bodies.flat_map(|body| body.instructions.iter()) {
        if let Instruction::DeclareFunction { function } = *instruction {
            named.entry(function).or_insert(false);
        }
    }
    named
}

#[cfg(test)]
mod tests {
    use super::Declarations;
    use crate::sound::test_support::{class, function, named, unit_of, world_unit};
    use phera_bytecode::facts::{DeclUniqueness, FunctionRef, UnitRef};
    use phera_bytecode::{ClassId, FunctionId, Instruction};
    use std::collections::BTreeSet;

    #[test]
    fn declarations_are_counted_across_files() {
        // a.php: function helper() {}  if (..) { function late() {} }
        //        function str_contains() {}   (a polyfill)
        //        class A {}  if (..) { class Maybe {} }
        let mut a = unit_of(vec![Instruction::DeclareFunction {
            function: FunctionId(1),
        }]);
        a.functions = vec![
            named("App\\helper", function(vec![])),
            named("late", function(vec![])),
            named("str_contains", function(vec![])),
            named("{closure:a.php:3}", function(vec![])),
        ];
        a.hoisted = vec![FunctionId(0), FunctionId(2)];
        a.classes = vec![class("App\\A", None), class("Maybe", None)];
        a.hoisted_classes = vec![ClassId(0)];
        // b.php: function helper() {}  class Maybe {}  class Countable {}
        let mut b = unit_of(vec![]);
        b.functions = vec![named("app\\HELPER", function(vec![]))];
        b.hoisted = vec![FunctionId(0)];
        b.classes = vec![class("maybe", None), class("Countable", None)];

        let units = vec![world_unit("a.php", a), world_unit("b.php", b)];
        let native_functions = BTreeSet::from(["str_contains".to_owned(), "strlen".to_owned()]);
        let native_classes = BTreeSet::from(["countable".to_owned()]);
        let declarations = Declarations::collect(&units, &native_functions, &native_classes);

        assert_eq!(
            declarations.function("app\\helper"),
            DeclUniqueness::Duplicate
        );
        assert_eq!(
            declarations.function("late"),
            DeclUniqueness::Unique {
                unit: UnitRef(0),
                hoisted: false
            }
        );
        assert_eq!(
            declarations.function("str_contains"),
            DeclUniqueness::Native
        );
        assert_eq!(declarations.function("strlen"), DeclUniqueness::Native);
        assert_eq!(declarations.function("missing"), DeclUniqueness::Unknown);
        // A closure is not a function PHP code can name.
        assert_eq!(
            declarations.function("{closure:a.php:3}"),
            DeclUniqueness::Unknown
        );
        assert_eq!(
            declarations.unique_function("late"),
            Some(FunctionRef {
                unit: UnitRef(0),
                function: 2
            })
        );
        assert_eq!(declarations.unique_function("app\\helper"), None);
        assert_eq!(declarations.unique_function("str_contains"), None);

        assert_eq!(
            declarations.class("app\\a"),
            DeclUniqueness::Unique {
                unit: UnitRef(0),
                hoisted: true
            }
        );
        assert_eq!(declarations.class("maybe"), DeclUniqueness::Duplicate);
        assert_eq!(declarations.class("countable"), DeclUniqueness::Native);
        assert_eq!(declarations.class("nope"), DeclUniqueness::Unknown);

        let table = declarations.function_table();
        let names: Vec<&str> = table.iter().map(|(name, _, _)| &**name).collect();
        assert_eq!(names, ["app\\helper", "late", "str_contains"]);
        assert_eq!(
            table[0].2,
            FunctionRef {
                unit: UnitRef(0),
                function: 1
            }
        );
    }
}
