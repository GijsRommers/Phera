--TEST--
Invalid debug info
--INI--
fatal_error_backtraces=1
--FILE--
<?php
class Example { public function __debugInfo() { return false; } }
register_shutdown_function(fn () => var_dump(error_get_last()['type']));
var_dump(new Example);
--EXPECTF--
Fatal error: __debuginfo() must return an array in %sinvalid-debug-info.php on line 4
Stack trace:
#0 %sinvalid-debug-info.php(4): var_dump(Object(Example))
#1 {main}
int(1)
