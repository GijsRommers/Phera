//! Exact listings make register, call-order, and path-table changes reviewable.

#[test]
fn loop_listing() {
    let unit = phera_compiler::compile(
        "<?php $index=0;while($index<3){$index++;}$finished=true;",
        "golden.php",
    )
    .expect("valid PHP");
    assert_eq!(unit.to_string(), include_str!("golden/loop.txt"));
}

#[test]
fn call_listing() {
    let unit = phera_compiler::compile(
        "<?php function identity($value){return $value;} $number=3;identity($number);",
        "golden.php",
    )
    .expect("valid PHP");
    assert_eq!(unit.to_string(), include_str!("golden/call.txt"));
}

#[test]
fn nested_listing() {
    let unit =
        phera_compiler::compile("<?php $grid=[];$grid[1][2]=3;", "golden.php").expect("valid PHP");
    assert_eq!(unit.to_string(), include_str!("golden/nested.txt"));
}

/// Nested exception edges remain innermost-first after IR target relocation.
#[test]
fn exception_handler_listing() {
    let unit = phera_compiler::compile(r#"<?php try { try { throw new Exception("x"); } catch (Error $error) { echo "error"; } finally { echo "inner"; } } catch (Throwable $error) { echo $error->getMessage(); } finally { echo "outer"; }"#, "golden.php").unwrap();
    assert_eq!(unit.to_string(), include_str!("golden/exceptions.txt"));
}
