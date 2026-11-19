--TEST--
Set time limit
--INI--
fatal_error_backtraces=1
--FILE--
<?php
echo 'CLI default: ', ini_get('max_execution_time'), "\n";
var_dump(set_time_limit(1));
try {
    while (true) {}
} catch (Throwable $e) {
    echo "caught\n";
} finally {
    echo "finally\n";
}
--EXPECTF--
CLI default: 0
bool(true)

Fatal error: Maximum execution time of 1 second exceeded in %sset-time-limit.php on line 5
Stack trace:
#0 {main}
