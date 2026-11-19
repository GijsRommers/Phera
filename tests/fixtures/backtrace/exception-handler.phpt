--TEST--
Exception handlers have a frame after main unwinds
--FILE--
<?php
set_exception_handler(function($error) { debug_print_backtrace(); });
throw new Exception('probe');
--EXPECTF--
#0 [internal function]: {closure:%s/exception-handler.php:2}(Object(Exception))
