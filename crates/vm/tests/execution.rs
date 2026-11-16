//! End-to-end register execution, bounded stacks, and amortised writes.
use phera_extension_api::Registry;
use phera_vm::Vm;
use std::sync::Arc;
async fn execute(source: &str, limit: usize) -> (i32, Vec<u8>) {
    let unit = Arc::new(phera_compiler::compile(source, "test.php").expect("valid PHP"));
    let mut vm = Vm::new(Arc::new(Registry::new()));
    vm.frame_limit = limit;
    let status = vm.run(unit).await;
    (status, vm.output)
}
#[tokio::test(flavor = "current_thread")]
async fn deep_chains() {
    for (op, term, expected) in [
        ("+", "1", "50000".to_owned()),
        (".", "'x'", "x".repeat(50_000)),
    ] {
        let source = format!(
            "<?php echo {};",
            std::iter::repeat_n(term, 50_000)
                .collect::<Vec<_>>()
                .join(op)
        );
        let (status, out) = execute(&source, 10).await;
        assert_eq!(status, 0);
        assert_eq!(out, expected.as_bytes());
    }
}
#[tokio::test(flavor = "current_thread")]
async fn deep_php_recursion() {
    let(status,out)=execute("<?php function descend($n){if($n==0)return 42; return descend($n-1);} echo descend(200000);",250_000).await;
    assert_eq!(status, 0);
    assert_eq!(out, b"42");
}
#[tokio::test(flavor = "current_thread")]
async fn recursion_limit_is_php_error() {
    let (status, out) = execute("<?php function forever(){forever();} forever();", 256).await;
    assert_eq!(status, 255);
    assert!(String::from_utf8_lossy(&out).contains("Maximum call stack depth of 256 exceeded"));
}
#[tokio::test(flavor = "current_thread")]
async fn async_native_callback_runs_on_same_vm() {
    use phera_extension_api::{
        Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Value,
    };
    fn callback<'a>(
        context: &'a mut dyn Context,
        args: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            let callable = args.first().cloned().unwrap_or_default();
            context.call(&callable, vec![Value::Int(21)]).await
        })
    }
    const PARAMS: &[Param] = &[Param::new("fn", ParamType::Mixed)];
    let mut registry = Registry::new();
    registry.function(FunctionEntry::asynchronous("callback", PARAMS, callback));
    let unit = Arc::new(
        phera_compiler::compile(
            "<?php function twice($x){return $x*2;}echo callback('twice');",
            "test.php",
        )
        .expect("valid PHP"),
    );
    let mut vm = Vm::new(Arc::new(registry));
    assert_eq!(vm.run(unit).await, 0);
    assert_eq!(vm.output, b"42");
}

#[tokio::test(flavor = "current_thread")]
async fn million_string_appends() {
    let (status, output) = execute(
        "<?php $text='';for($index=0;$index<1000000;$index++){$text.='x';}echo $text;",
        10,
    )
    .await;
    assert_eq!(status, 0);
    assert_eq!(output.len(), 1_000_000);
    assert!(output.iter().all(|byte| *byte == b'x'));
}

#[tokio::test(flavor = "current_thread")]
async fn million_array_pushes() {
    let (status,output) = execute("<?php $values=[];for($index=0;$index<1000000;$index++){$values[]=$index;}echo $values[999999];",10).await;
    assert_eq!((status, output), (0, b"999999".to_vec()));
}

#[tokio::test(flavor = "current_thread")]
async fn million_nested_array_writes() {
    let (status,output) = execute("<?php $grid=[];for($row=0;$row<1000;$row++){for($column=0;$column<1000;$column++){$grid[$row][$column]=$row+$column;}}echo $grid[999][999];",10).await;
    assert_eq!((status, output), (0, b"1998".to_vec()));
}
