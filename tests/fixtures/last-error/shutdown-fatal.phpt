--TEST--
A shutdown function can read the uncaught exception diagnostic
--FILE--
<?php
register_shutdown_function(function() {
    $last = error_get_last();
    echo $last['type'], ":", $last['message'], ":", $last['file'], ":", $last['line'], "\n";
});
throw new RuntimeException('broken');
--EXPECTF--
Fatal error: Uncaught RuntimeException: broken in %s/shutdown-fatal.php:6
Stack trace:
#0 {main}
  thrown in %s/shutdown-fatal.php on line 6
1:Uncaught RuntimeException: broken in %s/shutdown-fatal.php:6
Stack trace:
#0 {main}
  thrown:%s/shutdown-fatal.php:6
