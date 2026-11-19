--TEST--
Trigger handler and shutdown
--INI--
fatal_error_backtraces=1
zend.exception_string_param_max_len=6
--FILE--
<?php
trigger_error('previous');
set_error_handler(function ($level) {
    if ($level === E_USER_ERROR) {
        $last = error_get_last();
        echo 'handler: ', $last['message'], ' via ', $last['trace'][0]['function'], "\n";
        return false;
    }
    return true;
});
register_shutdown_function(function () {
    $last = error_get_last();
    echo 'shutdown: ', $last['type'], ' ', $last['message'], "\n";
    foreach ($last['trace'] as $frame) {
        echo $frame['function'], ' ', json_encode($frame['args']), "\n";
    }
    error_clear_last();
    var_dump(error_get_last());
});
function raise($argument) { trigger_error('fatal diagnostic', E_USER_ERROR); }
raise('long argument');
--EXPECTF--
Notice: previous in %strigger-handler-and-shutdown.php on line 2
handler: previous via trigger_error

Fatal error: fatal diagnostic in %strigger-handler-and-shutdown.php on line 20
Stack trace:
#0 %strigger-handler-and-shutdown.php(20): trigger_error('fatal ...', 256)
#1 %strigger-handler-and-shutdown.php(21): raise('long a...')
#2 {main}
shutdown: 256 fatal diagnostic
trigger_error ["fatal diagnostic",256]
raise ["long argument"]
NULL
