--TEST--
Trigger ignore args
--INI--
fatal_error_backtraces=1
zend.exception_ignore_args=1
--FILE--
<?php
register_shutdown_function(function () {
    foreach (error_get_last()['trace'] as $frame) {
        echo $frame['function'], ' args=', (int) isset($frame['args']), "\n";
    }
});
function raise($argument) { trigger_error('fatal', E_USER_ERROR); }
raise('secret');
--EXPECTF--
Deprecated: Passing E_USER_ERROR to trigger_error() is deprecated since 8.4, throw an exception or call exit with a string message instead in %strigger-ignore-args.php on line 7

Fatal error: fatal in %strigger-ignore-args.php on line 7
Stack trace:
#0 %strigger-ignore-args.php(7): trigger_error()
#1 %strigger-ignore-args.php(8): raise()
#2 {main}
trigger_error args=0
raise args=0
