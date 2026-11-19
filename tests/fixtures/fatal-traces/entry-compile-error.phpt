--TEST--
Entry compile error
--INI--
fatal_error_backtraces=1
--FILE--
<?php
function no_result(): void { return 3; }
--EXPECTF--
Fatal error: A void function must not return a value in %sentry-compile-error.php on line 2
Stack trace:
#0 {main}
