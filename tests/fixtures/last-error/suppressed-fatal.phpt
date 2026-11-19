--TEST--
A suppressed user fatal still terminates and is recorded
--FILE--
<?php
register_shutdown_function(function() {
    $last = error_get_last();
    echo $last['type'], ':', $last['message'], "\n";
});
@trigger_error('still fatal', E_USER_ERROR);
--EXPECTF--
Fatal error: still fatal in %s/suppressed-fatal.php on line 6
256:still fatal
