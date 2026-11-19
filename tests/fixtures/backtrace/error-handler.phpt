--TEST--
Error handlers called from a native warning have no PHP call site
--FILE--
<?php
set_error_handler(function($level, $message) {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    return true;
});
trigger_error('probe');
--EXPECTF--
#0 [internal function]: {closure:%s/error-handler.php:2}()
#1 %s/error-handler.php(6): trigger_error()
