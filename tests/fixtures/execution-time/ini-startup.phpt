--TEST--
Ini startup
--INI--
max_execution_time=1
fatal_error_backtraces=1
--FILE--
<?php
ob_start();
echo "buffered\n";
register_shutdown_function(function () {
    $last = error_get_last();
    echo 'shutdown: ', $last['type'], ' ', $last['trace'][0]['function'], "\n";
});
function spin() { while (true) {} }
spin();
--EXPECTF--
buffered

Fatal error: Maximum execution time of 1 second exceeded in %sini-startup.php on line 8
Stack trace:
#0 %sini-startup.php(9): spin()
#1 {main}
shutdown: 1 spin
