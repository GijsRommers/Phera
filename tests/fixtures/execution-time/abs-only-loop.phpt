--TEST--
A loop containing only abs still times out
--INI--
fatal_error_backtraces=0
--FILE--
<?php
$i = -7;
var_dump(set_time_limit(1));
while (true) { abs($i); }
--EXPECTF--
bool(true)

Fatal error: Maximum execution time of 1 second exceeded in %sabs-only-loop.php on line 4
