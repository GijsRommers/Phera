//! Recursive includes use the same bounded heap stack as PHP calls.

use phera_bytecode::CompiledUnit;
use phera_extension_api::{LocalFuture, Registry};
use phera_vm::{CodeLoader, Vm};
use std::{path::PathBuf, sync::Arc};

/// Reuse one compiled unit to model an endlessly self-including file.
struct RecursiveLoader {
    unit: Arc<CompiledUnit>,
    loads: std::sync::atomic::AtomicUsize,
}

impl CodeLoader for RecursiveLoader {
    fn load(&self, _path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, String>> {
        Box::pin(async {
            if self
                .loads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                >= 32
            {
                return Err("bounded regression loader stopped".into());
            }
            Ok(self.unit.clone())
        })
    }
}

/// Include frames must stop before allocating an unbounded register stack.
#[tokio::test(flavor = "current_thread")]
async fn recursive_includes_obey_frame_limit() {
    let unit = Arc::new(phera_compiler::compile("<?php include 'self.php';", "self.php").unwrap());
    let mut vm = Vm::new(Arc::new(Registry::new()));
    vm.frame_limit = 8;
    vm.set_loader(Arc::new(RecursiveLoader {
        unit: unit.clone(),
        loads: std::sync::atomic::AtomicUsize::new(0),
    }));
    assert_eq!(vm.run(unit).await, 255);
    assert!(String::from_utf8_lossy(&vm.output).contains("Maximum call stack depth of 8 exceeded"));
}

/// Loader failures preserve PHP's stream reason rather than collapsing permission errors to ENOENT.
struct PermissionLoader;

impl CodeLoader for PermissionLoader {
    fn load(&self, _path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, String>> {
        Box::pin(async { Err("Permission denied (os error 13)".into()) })
    }
}

/// Required includes terminate with 255; optional includes warn twice and continue.
#[tokio::test(flavor = "current_thread")]
async fn unreadable_includes_report_permission_denied() {
    for construct in ["include", "include_once", "require", "require_once"] {
        let source = format!("<?php {construct} 'unreadable.php';");
        let unit = Arc::new(phera_compiler::compile(&source, "test.php").unwrap());
        let mut vm = Vm::new(Arc::new(Registry::new()));
        vm.set_loader(Arc::new(PermissionLoader));
        let required = construct.starts_with("require");
        assert_eq!(vm.run(unit).await, if required { 255 } else { 0 });
        let output = String::from_utf8_lossy(&vm.output);
        assert!(output.contains(&format!(
            "{construct}(unreadable.php): Failed to open stream: Permission denied"
        )));
        assert!(output.contains("include_path='.'"));
    }
}

/// Record failed resolutions so explicit relative paths cannot accidentally use the script directory.
struct RecordingLoader {
    paths: Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl CodeLoader for RecordingLoader {
    fn load(&self, path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, String>> {
        self.paths.lock().unwrap().push(path);
        Box::pin(async { Err("No such file or directory".into()) })
    }
}

/// PHP searches the script directory only for names without an absolute or explicit relative prefix.
#[tokio::test(flavor = "current_thread")]
async fn include_resolution_attempts_match_php() {
    for (name, expected) in [
        ("./target.php", vec!["./target.php"]),
        ("../target.php", vec!["../target.php"]),
        ("/target.php", vec!["/target.php"]),
        ("target.php", vec!["target.php", "scripts/target.php"]),
    ] {
        let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
        let source = format!("<?php @include '{name}';");
        let unit = Arc::new(phera_compiler::compile(&source, "scripts/main.php").unwrap());
        let mut vm = Vm::new(Arc::new(Registry::new()));
        vm.set_loader(Arc::new(RecordingLoader {
            paths: paths.clone(),
        }));
        assert_eq!(vm.run(unit).await, 0);
        let expected: Vec<_> = expected.into_iter().map(PathBuf::from).collect();
        assert_eq!(*paths.lock().unwrap(), expected);
    }
}
