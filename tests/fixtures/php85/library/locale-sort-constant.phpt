--TEST--
SORT_LOCALE_STRING remains available with a read-time deprecation
--FILE--
<?php
set_error_handler(function ($severity, $message) { echo $severity, ': ', $message, PHP_EOL; });
var_dump(defined('SORT_LOCALE_STRING'), SORT_LOCALE_STRING);
--EXPECT--
bool(true)
int(5)
