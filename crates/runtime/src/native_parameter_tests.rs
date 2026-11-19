//! Compare runtime registration with the declarations generated from PHP's stubs.
//! For example, `preg_match()` takes its third argument by reference.

use phera_extension_api::{Param, Registry, native_parameters::signature};

#[test]
fn native_parameter_tables_match_php_stubs() {
    let runtime = super::Runtime::new();
    let mut audit = Audit::default();
    audit.registry(&runtime.web_registry);
    audit.messages.sort_unstable();
    audit.missing.sort_unstable();
    eprintln!(
        "Compared {} functions and {} methods",
        audit.functions, audit.methods
    );
    eprintln!(
        "Without php-src stubs ({}): {}",
        audit.missing.len(),
        audit.missing.join(", ")
    );
    assert!(audit.messages.is_empty(), "{}", audit.messages.join("\n"));
}

#[derive(Default)]
struct Audit {
    functions: usize,
    methods: usize,
    missing: Vec<String>,
    messages: Vec<String>,
}

impl Audit {
    fn registry(&mut self, registry: &Registry) {
        for function in registry.functions() {
            if self.compare(function.name, function.params) {
                self.functions += 1;
            }
        }
        for class in registry.classes() {
            for method in class.methods {
                if self.compare(&format!("{}::{}", class.name, method.name), method.params) {
                    self.methods += 1;
                }
            }
        }
    }

    fn compare(&mut self, name: &str, parameters: &[Param]) -> bool {
        let Some(stub) = signature(name) else {
            self.missing.push(name.to_owned());
            return false;
        };
        let expected: Vec<_> = stub.parameters().collect();
        let actual: Vec<_> = parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name,
                    parameter.optional,
                    parameter.by_ref,
                    parameter.variadic,
                )
            })
            .collect();
        let declared: Vec<_> = expected
            .iter()
            .map(|parameter| {
                (
                    parameter.name,
                    parameter.optional,
                    parameter.by_ref,
                    parameter.variadic,
                )
            })
            .collect();
        if actual != declared {
            self.messages.push(format!(
                "{name} ({}):\n  registered {actual:?}\n  PHP        {declared:?}",
                stub.source
            ));
        }
        true
    }
}
