--TEST--
A loop containing only strlen still times out
--INI--
fatal_error_backtraces=0
--FILE--
<?php
$s = 'test';
var_dump(set_time_limit(1));
while (true) { strlen($s); }
--EXPECTF--
bool(true)

Fatal error: Maximum execution time of 1 second exceeded in %sstrlen-only-loop.php on line 4
