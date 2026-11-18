//! Destructors where PHP cannot be the oracle: a chain PHP itself overflows
//! its stack on, and a request stopped in the middle.
//!
//! ```php
//! for ($i = 0; $i < 100000; $i++) { $node = new Node; $node->next = $head; $head = $node; }
//! $head = null;   // PHP 8.5 crashes here; Phera destroys the chain iteratively
//! ```

use std::sync::Arc;

use phera_extension_api::Registry;

use crate::Vm;

#[tokio::test(flavor = "current_thread")]
async fn a_chain_of_100000_objects_with_destructors_is_destroyed_without_recursion() {
    let source = r#"<?php
        class Node { public static int $destroyed = 0; public $next; function __destruct() { self::$destroyed++; } }
        $head = null;
        for ($i = 0; $i < 100000; $i++) { $node = new Node; $node->next = $head; $head = $node; }
        unset($node);
        $head = null;
        echo Node::$destroyed;
    "#;
    let unit = phera_compiler::compile(source, "long-chain.php").unwrap();
    let mut vm = Vm::new(Arc::new(Registry::new()));
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    assert_eq!(vm.output, b"100000");
}

/// The server stops a request that runs too long by dropping its task, so
/// the end of the request never runs. Its objects (here a cycle and objects
/// with destructors) must all be freed with the `Vm`, without destructors,
/// so that nothing of the request outlives it.
#[tokio::test(flavor = "current_thread")]
async fn a_stopped_request_frees_every_object_without_destructors() {
    let source = r#"<?php
        class D { public $other; function __destruct() { echo "destructor"; } }
        $a = new D; $b = new D; $a->other = $b; $b->other = $a;
        $c = new D;
        $list = [new D, new D];
        while (true) {}
    "#;
    let unit = phera_compiler::compile(source, "stopped.php").unwrap();
    let mut vm = Vm::new(Arc::new(Registry::new()));
    let stopped =
        tokio::time::timeout(std::time::Duration::from_millis(50), vm.run(Arc::new(unit))).await;
    assert!(stopped.is_err(), "the script never ends by itself");
    let objects: Vec<_> = vm.classes.objects.clone();
    assert_eq!(
        objects
            .iter()
            .filter(|object| object.upgrade().is_some())
            .count(),
        5
    );
    assert!(vm.output.is_empty());
    drop(vm);
    assert!(objects.iter().all(|object| object.upgrade().is_none()));
}
