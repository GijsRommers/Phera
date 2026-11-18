<?php
// Reproducer: `===` on an array that contains a reference to itself overflows
// the Rust stack of the main thread and aborts the process (exit 134,
// "thread 'main' has overflowed its stack"). Real PHP 8.5 prints bool(true).
// Command: phera tests/security/strict-identity-cycle-crash.php
$a = [];
$a["s"] = &$a;
var_dump($a === $a);
