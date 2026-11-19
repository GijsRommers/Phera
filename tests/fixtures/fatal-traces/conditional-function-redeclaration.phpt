--TEST--
Conditional function redeclaration
--INI--
fatal_error_backtraces=1
--FILE--
<?php
register_shutdown_function(fn () => var_dump(error_get_last()['type']));
function define_inner() { function inner() {} }
define_inner();
define_inner();
--EXPECTF--
Fatal error: Cannot redeclare function inner() (previously declared in %sconditional-function-redeclaration.php:3) in %sconditional-function-redeclaration.php on line 3
Stack trace:
#0 %sconditional-function-redeclaration.php(5): define_inner()
#1 {main}
int(1)
