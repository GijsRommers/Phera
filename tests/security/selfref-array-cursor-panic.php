<?php
// Reproducer: end()/reset()/array_pop()/array_shift() on an array that
// contains a reference to itself panic the process:
//   thread 'main' panicked at crates/value/src/value.rs:53:57:
//   RefCell already mutably borrowed   (exit 101, reproduced 3/3)
// Real PHP 8.5 prints "ok". The cursor functions hold the reference cell's
// try_borrow_mut guard while calling Value::deref() on the element, which
// borrows the same cell again.
// Command: phera tests/security/selfref-array-cursor-panic.php
$s = ["self" => &$s];
end($s);
echo "ok\n";
