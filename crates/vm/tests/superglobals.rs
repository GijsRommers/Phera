//! Request-injected superglobals remain visible across function and include boundaries.

use std::{path::PathBuf, sync::Arc};

use phera_bytecode::CompiledUnit;
use phera_extension_api::{LocalFuture, Registry};
use phera_value::{ArrayKey, PhpArray, Value};
use phera_vm::{CodeLoadError, CodeLoader, Vm};

/// An in-memory include exercises the public embedding API without filesystem dependencies.
struct IncludeLoader;

impl CodeLoader for IncludeLoader {
    fn load(&self, _path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, CodeLoadError>> {
        Box::pin(async {
            phera_compiler::compile("<?php echo $_GET['x']; $_SESSION['k'] = 7;", "shared.php")
                .map(Arc::new)
                .map_err(CodeLoadError::Compile)
        })
    }
}

/// Every request array is a global in ordinary and nested functions, without global declarations.
#[tokio::test(flavor = "current_thread")]
async fn injected_arrays_in_all_scopes() {
    for name in [
        "_GET", "_POST", "_COOKIE", "_SERVER", "_REQUEST", "_FILES", "_ENV", "_SESSION",
    ] {
        let source = format!(
            "<?php function outer() {{ function inner() {{ echo ${name}['x']; ${name}['x'] = 9; }} echo ${name}['x']; inner(); }} outer(); echo ${name}['x'];"
        );
        let unit = Arc::new(phera_compiler::compile(&source, "test.php").expect("valid source"));
        let mut vm = Vm::new(Arc::new(Registry::new()));
        let mut array = PhpArray::new();
        array.insert(ArrayKey::str("x"), Value::Int(4));
        vm.set_global(name, Value::Array(std::rc::Rc::new(array)));
        assert_eq!(vm.run(unit).await, 0, "{name}");
        assert_eq!(vm.output, b"449", "{name}");
    }
}

/// Includes inside functions must share request cells rather than initialize local copies.
#[tokio::test(flavor = "current_thread")]
async fn included_file_shares_superglobals() {
    let source = "<?php function run() { include 'shared.php'; echo $_SESSION['k']; } run(); echo $_SESSION['k'];";
    let unit = Arc::new(phera_compiler::compile(source, "test.php").expect("valid source"));
    let mut vm = Vm::new(Arc::new(Registry::new()));
    let mut array = PhpArray::new();
    array.insert(ArrayKey::str("x"), Value::Int(4));
    vm.set_global("_GET", Value::Array(std::rc::Rc::new(array)));
    vm.set_global("_SESSION", Value::Array(std::rc::Rc::new(PhpArray::new())));
    vm.set_loader(Arc::new(IncludeLoader));
    assert_eq!(vm.run(unit).await, 0);
    assert_eq!(vm.output, b"477");
}
