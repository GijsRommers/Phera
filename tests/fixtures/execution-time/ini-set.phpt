--TEST--
Ini set
--INI--
fatal_error_backtraces=1
--FILE--
<?php
var_dump(ini_set('max_execution_time', '1'));
while (true) {}
--EXPECTF--
string(1) "0"

Fatal error: Maximum execution time of 1 second exceeded in %sini-set.php on line 3
Stack trace:
#0 {main}
