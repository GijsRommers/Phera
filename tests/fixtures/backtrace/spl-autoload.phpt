--TEST--
The autoload callback is framed by the native caller that looked up the class
--FILE--
<?php
spl_autoload_register(function ($class) {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    echo $class, "\n";
});
class_exists('PheraTraceAbsent');
--EXPECTF--
#0 [internal function]: {closure:%s/spl-autoload.php:2}()
#1 %s/spl-autoload.php(6): class_exists()
PheraTraceAbsent
