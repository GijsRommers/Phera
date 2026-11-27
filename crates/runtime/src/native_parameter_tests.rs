//! Compare runtime registration with the declarations generated from PHP's stubs.
//! For example, `preg_match()` takes its third argument by reference.

mod reflection;

use phera_extension_api::{Param, Registry, native_parameters::signature};

#[test]
fn native_parameter_tables_match_php_stubs() {
    let runtime = super::Runtime::new();
    let grpc = stubs(phera_ext_grpc::STUBS, "grpc/stubs.php");
    let phera_stubs = [
        (
            "Phera\\WebSocket\\",
            "websocket/stubs.php",
            stubs(include_str!("websocket/stubs.php"), "websocket/stubs.php"),
        ),
        (
            "Phera\\Http\\Client",
            "http_client/stubs.php",
            stubs(
                include_str!("http_client/stubs.php"),
                "http_client/stubs.php",
            ),
        ),
        (
            "Phera\\Log\\",
            "log/stubs.php",
            stubs(include_str!("log/stubs.php"), "log/stubs.php"),
        ),
        (
            "Phera\\Http\\",
            "http_server/stubs.php",
            stubs(
                include_str!("http_server/stubs.php"),
                "http_server/stubs.php",
            ),
        ),
        ("Phera\\Grpc\\", "grpc/stubs.php", grpc.clone()),
        ("Phera\\Protobuf\\", "grpc/stubs.php", grpc),
    ];
    let mut audit = Audit {
        functions: 0,
        methods: 0,
        missing: Vec::new(),
        messages: Vec::new(),
        phera_stubs: &phera_stubs,
    };
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
            "InflateContext::__construct",
            "OpenSSLAsymmetricKey::__construct",
            "OpenSSLCertificate::__construct",
            "OpenSSLCertificateSigningRequest::__construct",
            "PgSql\\Connection::__construct",
            "PgSql\\Lob::__construct",
            "PgSql\\Result::__construct",
            "XMLParser::__construct",
        ],
        "These private constructors have no declaration in PHP's stubs",
    );
    assert!(audit.messages.is_empty(), "{}", audit.messages.join("\n"));
}

type StubSignature = Vec<(String, bool, bool, bool)>;

/// The declarations of one of Phera's own namespaces, which PHP's stubs do
/// not have: the name prefix, the stub file, and its signatures.
type PheraStubs = (
    &'static str,
    &'static str,
    std::collections::HashMap<String, StubSignature>,
);

fn stubs(source: &str, file: &str) -> std::collections::HashMap<String, StubSignature> {
    let unit = phera_compiler::compile(source, file).expect("Phera stubs compile");
    let extract = |params: &[phera_bytecode::Parameter]| -> StubSignature {
        params
            .iter()
            .map(|p| {
                let name = unit.names[p.name.index()].clone();
                (
                    name,
                    p.has_default || p.variadic,
                    p.by_reference,
                    p.variadic,
                )
            })
            .collect()
    };
    let mut map = std::collections::HashMap::new();
    for function in &unit.functions {
        if !function.name.contains('{') {
            map.insert(function.name.clone(), extract(&function.parameters));
        }
    }
    for class in &unit.classes {
        for method in &class.methods {
            let method_name = &unit.names[method.name.index()];
            let function = &unit.functions[method.function.index()];
            map.insert(
                format!("{}::{method_name}", class.name),
                extract(&function.parameters),
            );
        }
    }
    map
}

struct Audit<'a> {
    functions: usize,
    methods: usize,
    missing: Vec<String>,
    messages: Vec<String>,
    phera_stubs: &'a [PheraStubs],
}

impl Audit<'_> {
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
        if let Some((_, file, stubs)) = self
            .phera_stubs
            .iter()
            .find(|(prefix, _, _)| name.starts_with(prefix))
        {
            let Some(declared) = stubs.get(name) else {
                self.messages
                    .push(format!("{name}: missing declaration in {file}"));
                return false;
            };
            let actual: StubSignature = parameters
                .iter()
                .map(|parameter| {
                    (
                        parameter.name.to_owned(),
                        parameter.optional,
                        parameter.by_ref,
                        parameter.variadic,
                    )
                })
                .collect();
            if &actual != declared {
                self.messages.push(format!(
                    "{name} ({file}):\n  registered {actual:?}\n  stubs      {declared:?}"
                ));
            }
            return true;
        }
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
