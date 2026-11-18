//! Storage tests inspect live stack boundaries rather than inferring cleanup from PHP output.

use std::sync::Arc;

use phera_bytecode::CompiledUnit;
use phera_extension_api::{ClassEntry, FunctionEntry, Registry};
use phera_value::Value;

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use crate::calls::frame::Frame;

/// A native checkpoint stops the interpreter without introducing another PHP frame.
fn test_vm(source: &str) -> (Vm, Arc<CompiledUnit>) {
    let mut registry = Registry::new();
    registry.class(ClassEntry::interface("Throwable"));
    registry.class(ClassEntry {
        interfaces: &["Throwable"],
        ..ClassEntry::class("Exception")
    });
    registry.class(ClassEntry {
        interfaces: &["Throwable"],
        ..ClassEntry::class("Error")
    });
    registry.function(FunctionEntry::sync("checkpoint", &[], |_, _| {
        Ok(Value::Null)
    }));
    let mut vm = Vm::new(Arc::new(registry));
    let unit = Arc::new(phera_compiler::compile(source, "windows.php").unwrap());
    let main = vm.code.load(&unit);
    for function in &unit.hoisted {
        vm.function_table.insert(
            unit.functions[function.index()].name.to_ascii_lowercase(),
            vm.code.declared(main, function.index()),
        );
    }
    vm.registers.grow_to(unit.main.registers as usize);
    vm.call_stack.push(Frame::new(main, 0, None));
    (vm, unit)
}

/// Reach the next checkpoint after applying the same unwind algorithm as the async shell.
async fn checkpoint(vm: &mut Vm) -> (usize, usize, usize) {
    let mut work = 0;
    loop {
        match vm.run_batch(0, &mut work) {
            Ok(BatchExit::NativeCall(destination)) => {
                let checkpoint = vm
                    .call_stack
                    .top_details()
                    .and_then(|details| details.callee.as_ref())
                    .is_some_and(|callee| callee.name.as_ref() == "checkpoint");
                let value = vm.finish_native_window().await.unwrap();
                vm.set(destination, value);
                if checkpoint {
                    return (
                        vm.registers.len(),
                        vm.call_stack.call_depth(),
                        vm.call_stack.depth(),
                    );
                }
            }
            Ok(BatchExit::Yield) => tokio::task::yield_now().await,
            Ok(BatchExit::Instruction(instruction)) => {
                if let Err(error) = vm.dispatch(instruction, 0).await {
                    vm.handle_exception(error, 0).unwrap();
                }
            }
            Err(error) => vm.handle_exception(error, 0).unwrap(),
            _ => panic!("checkpoint was not reached"),
        }
    }
}

/// Ten million ordinary calls must return every live stack to the caller's starting size.
#[tokio::test(flavor = "current_thread")]
async fn ten_million_calls_release_their_windows() {
    let source = "<?php function identity($x){return $x;} checkpoint(); \
        for($i=0;$i<10000000;$i++){identity($i);} checkpoint();";
    let (mut vm, _unit) = test_vm(source);
    let starting = checkpoint(&mut vm).await;
    assert_eq!(checkpoint(&mut vm).await, starting);
    assert_eq!(vm.interpret(0).await.unwrap(), Value::Int(1));
    assert_eq!(
        (
            vm.registers.len(),
            vm.call_stack.call_depth(),
            vm.call_stack.depth()
        ),
        (0, 0, 0)
    );
}

/// A throw in the fifth nested argument discards all five unfinished windows in the catch frame.
#[tokio::test(flavor = "current_thread")]
async fn fifth_nested_argument_releases_preparations() {
    let source = "<?php function f($a,$b){return $b;} function fail(){throw new Exception;} \
        checkpoint(); try{f(1,f(2,f(3,f(4,f(5,fail())))));}catch(Throwable){checkpoint();}";
    let (mut vm, _unit) = test_vm(source);
    let starting = checkpoint(&mut vm).await;
    assert_eq!(checkpoint(&mut vm).await, starting);
}

/// A hundred thousand PHP activations return iteratively, below the public frame limit.
#[tokio::test(flavor = "current_thread")]
async fn hundred_thousand_calls_do_not_use_the_rust_stack() {
    let source = "<?php function descend($n){return $n?descend($n-1):7;} \
        checkpoint(); $result=descend(100000); checkpoint();";
    let (mut vm, _unit) = test_vm(source);
    let starting = checkpoint(&mut vm).await;
    assert_eq!(checkpoint(&mut vm).await, starting);
    assert!(vm.call_stack.frame_capacity() >= 100_000);
}

/// Entry-limit failure leaves the catching frame usable, with no abandoned window.
#[tokio::test(flavor = "current_thread")]
async fn frame_limit_failure_can_be_caught_and_retried() {
    let source = "<?php function descend($n){return $n?descend($n-1):7;} \
        checkpoint(); try{descend(100000);}catch(Throwable){checkpoint();} \
        descend(3); checkpoint();";
    let (mut vm, _unit) = test_vm(source);
    vm.frame_limit = 100_000;
    let starting = checkpoint(&mut vm).await;
    assert_eq!(checkpoint(&mut vm).await, starting);
    assert_eq!(checkpoint(&mut vm).await, starting);
}
