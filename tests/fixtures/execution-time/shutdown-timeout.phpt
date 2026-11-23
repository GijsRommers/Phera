--TEST--
Shutdown timeout
--INI--
fatal_error_backtraces=1
--FILE--
<?php
function spin() { while (true) {} }
register_shutdown_function('spin');
set_time_limit(1);
spin();
--EXPECTF--
Fatal error: Maximum execution time of 1 second exceeded in %sshutdown-timeout.php on line 2
Stack trace:
#0 %sshutdown-timeout.php(5): spin()
#1 {main}

Fatal error: Maximum execution time of 1 second exceeded in %sshutdown-timeout.php on line 2
Stack trace:
#0 [internal function]: spin()
#1 {main}
