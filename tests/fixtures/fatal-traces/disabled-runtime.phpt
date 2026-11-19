--TEST--
Disabled runtime
--INI--
fatal_error_backtraces=1
--FILE--
<?php
ini_set('fatal_error_backtraces', '0');
register_shutdown_function(fn () => var_dump(isset(error_get_last()['trace'])));
eval('class Repeated {} class Repeated {}');
--EXPECTF--
Fatal error: Cannot redeclare class Repeated (previously declared in %sdisabled-runtime.php(4) : eval()'d code:1) in %sdisabled-runtime.php(4) : eval()'d code on line 1
bool(false)
