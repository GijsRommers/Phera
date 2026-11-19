//! Every registered function has PHP's parameter names, which named
//! arguments (`str_pad(string: "x", length: 3)`) depend on, and PHP's
//! by-reference, optional and variadic parameters.

use phera_ext_standard::Standard;
use phera_extension_api::testing::oracle;
use phera_extension_api::{Extension, Registry};

#[test]
fn signatures_match_php() {
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    let mut functions: Vec<_> = registry.functions().collect();
    functions.sort_by_key(|entry| entry.name);
    let mut script = String::from("<?php\n");
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for entry in functions {
        let name = entry.name;
        script.push_str(&format!(
            "$f = new ReflectionFunction('{name}'); $p = [];\n\
             foreach ($f->getParameters() as $x) {{ $p[] = ($x->isPassedByReference() ? '&' : '') \
             . ($x->isVariadic() ? '...' : '') . $x->getName() . ($x->isOptional() && !$x->isVariadic() ? '?' : ''); }}\n\
             echo '{name}(', implode(', ', $p), \")\\n\";\n"
        ));
        let params: Vec<String> = entry
            .params
            .iter()
            .map(|param| {
                format!(
                    "{}{}{}{}",
                    if param.by_ref { "&" } else { "" },
                    if param.variadic { "..." } else { "" },
                    param.name,
                    if param.optional && !param.variadic {
                        "?"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        rust.push(format!("{name}({})", params.join(", ")));
        labels.push(name.to_string());
    }
    let php = oracle::run_php("standard_signatures", &script).expect("PHP oracle required");
    oracle::compare("standard signatures", &labels, &php, &rust);
}
