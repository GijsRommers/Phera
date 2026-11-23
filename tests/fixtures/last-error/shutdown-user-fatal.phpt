--TEST--
Shutdown sees E_USER_ERROR after its deprecation
--FILE--
<?php
register_shutdown_function(function() {
    $last = error_get_last();
    echo $last['type'], ":", $last['message'], ":", $last['line'], "\n";
});
trigger_error('failed request', E_USER_ERROR);
--EXPECTF--
Deprecated: Passing E_USER_ERROR to trigger_error() is deprecated since 8.4, throw an exception or call exit with a string message instead in %s/shutdown-user-fatal.php on line 6

Fatal error: failed request in %s/shutdown-user-fatal.php on line 6
256:failed request:6
