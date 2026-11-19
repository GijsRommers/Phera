--TEST--
Trigger handled and cleared
--INI--
fatal_error_backtraces=1
--FILE--
<?php
trigger_error('previous');
set_error_handler(function ($level) {
    if ($level === E_USER_ERROR) {
        echo error_get_last()['trace'][0]['args'][0], "\n";
    }
    return true;
});
trigger_error('handled', E_USER_ERROR);
echo error_get_last()['message'], "\n";
trigger_error('also handled');
echo (int) isset(error_get_last()['trace']), "\n";
--EXPECTF--
Notice: previous in %strigger-handled-and-cleared.php on line 2
handled
previous
0
