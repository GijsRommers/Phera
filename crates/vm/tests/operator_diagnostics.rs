//! Intrinsic conversion diagnostics survive both normal and exceptional returns.

use phera_extension_api::Registry;
use phera_vm::Vm;
use std::sync::Arc;

/// Quiet dynamic reads suppress missing-variable diagnostics in every quiet construct.
#[tokio::test(flavor = "current_thread")]
async fn dynamic_quiet_reads() {
    let unit = Arc::new(
        phera_compiler::compile(
            "<?php $name='absent'; $$name ?? 1; isset($$name); empty($$name);",
            "test.php",
        )
        .unwrap(),
    );
    let mut vm = Vm::new(Arc::new(Registry::new()));
    assert_eq!(vm.run(unit).await, 0);
    assert!(vm.output.is_empty());
}
