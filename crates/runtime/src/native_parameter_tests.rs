//! Compare runtime registration with the declarations generated from PHP's stubs.
//! For example, `preg_match()` takes its third argument by reference.

mod reflection;

use phera_extension_api::{Param, Registry, native_parameters::signature};

#[test]
fn native_parameter_tables_match_php_stubs() {
    let runtime = super::Runtime::new();
    let mut audit = Audit::default();
    audit.registry(&runtime.web_registry);
    for function in runtime.registry.functions() {
        if runtime.web_registry.get_function(function.name).is_none()
            && audit.compare(function.name, function.params)
        {
            audit.functions += 1;
        }
    }
    audit.messages.sort_unstable();
    audit.missing.sort_unstable();
    eprintln!(
        "Compared {} functions and {} methods",
        audit.functions, audit.methods
    );
    assert_eq!(
        audit.missing,
        [
            "CurlHandle::__construct",
            "CurlMultiHandle::__construct",
            "CurlShareHandle::__construct",
            "CurlSharePersistentHandle::__construct",
            "DeflateContext::__construct",
            "Directory::__construct",
            "GdImage::__construct",
            "InflateContext::__construct",
            "OpenSSLAsymmetricKey::__construct",
            "OpenSSLCertificate::__construct",
            "OpenSSLCertificateSigningRequest::__construct",
            "PgSql\\Connection::__construct",
            "PgSql\\Lob::__construct",
            "PgSql\\Result::__construct",
        ],
        "These private constructors have no declaration in PHP's stubs",
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
            assert!(
                parameters.is_empty(),
                "{name} has parameters without a PHP stub"
            );
            return false;
        };
        let expected: Vec<_> = stub.parameters().collect();
        for parameter in &expected {
            if parameter.optional
                && !parameter.variadic
                && parameter.expression != "UNKNOWN"
                && parameter.default().is_none()
            {
                self.messages.push(format!(
                    "{name} ({}): missing value for ${} = {}",
                    stub.source, parameter.name, parameter.expression
                ));
            }
        }
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
